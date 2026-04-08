use anyhow::Context;
use dashmap::DashMap;
use gcp_auth::TokenProvider;
use html_escape::decode_html_entities;
use language_utils::Language;
use std::path::PathBuf;
use std::sync::Arc;
use xxhash_rust::xxh3::xxh3_64;

pub struct GoogleTranslator {
    client: reqwest::Client,
    source_language: String,
    target_language: String,
    auth: Arc<dyn TokenProvider>,
    project_id: String,
    cache: DashMap<u64, String>, // hash -> translation
    cache_dir: PathBuf,
    master_cache_file: PathBuf,
}

impl GoogleTranslator {
    pub async fn new(
        source_language: Language,
        target_language: Language,
        cache_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        let creds_path = std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
            .context("GOOGLE_APPLICATION_CREDENTIALS not set")?;
        let creds_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&creds_path)?)?;
        let project_id = creds_json["project_id"]
            .as_str()
            .context("No project_id in service account JSON")?
            .to_string();

        let auth = gcp_auth::provider()
            .await
            .context("Failed to initialize Google Cloud auth")?;

        std::fs::create_dir_all(&cache_dir)?;

        let master_cache_file = cache_dir.join("master_cache.json");
        let cache: DashMap<u64, String> = if master_cache_file.exists() {
            let master_content = std::fs::read_to_string(&master_cache_file)?;
            serde_json::from_str(&master_content).unwrap_or_default()
        } else {
            DashMap::new()
        };

        let res = Self {
            client: reqwest::Client::new(),
            source_language: source_language.iso_639_1().to_string(),
            target_language: target_language.iso_639_1().to_string(),
            auth,
            project_id,
            cache,
            cache_dir,
            master_cache_file,
        };
        res.consolidate_cache();
        Ok(res)
    }

    async fn get_token(&self) -> anyhow::Result<String> {
        let token = self
            .auth
            .token(&["https://www.googleapis.com/auth/cloud-translation"])
            .await
            .context("Failed to get access token")?;
        Ok(token.as_str().to_string())
    }

    pub async fn translate(&self, text: &str) -> anyhow::Result<String> {
        // Compute hash for this text
        let hash_input = format!("{}::{}::{text}", self.source_language, self.target_language);
        let hash = xxh3_64(hash_input.as_bytes());

        // Check in-memory cache (includes master cache loaded on startup)
        if let Some(t) = self.cache.get(&hash) {
            return Ok(t.clone());
        }

        // Not in cache - make API call
        let cache_file = self.cache_dir.join(format!("{hash}.json"));

        let token = self.get_token().await?;
        let url = format!(
            "https://translation.googleapis.com/v3/projects/{}/locations/us-central1:translateText",
            self.project_id
        );
        let model = format!(
            "projects/{}/locations/us-central1/models/general/translation-llm",
            self.project_id
        );
        let resp = self
            .client
            .post(url)
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "sourceLanguageCode": self.source_language,
                "targetLanguageCode": self.target_language,
                "contents": [text],
                "mimeType": "text/plain",
                "model": model,
            }))
            .send()
            .await
            .context("Failed to call Google Translate API")?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .context("Failed to read Google Translate response")?;

        if !status.is_success() {
            anyhow::bail!("Google Translate API error ({status}) for '{text}': {body}");
        }

        let value: serde_json::Value =
            serde_json::from_str(&body).context("Failed to parse Google Translate response")?;
        let translated = value["translations"][0]["translatedText"]
            .as_str()
            .map(|s| decode_html_entities(s).to_string());

        match translated {
            Some(t) if !t.trim().is_empty() => {
                self.cache.insert(hash, t.clone());
                // Write individual cache file with just the translation
                tokio::fs::write(&cache_file, &t).await?;
                Ok(t)
            }
            _ => {
                // Don't cache empty/failed translations so they can be retried
                anyhow::bail!(
                    "Google Translate returned empty result for '{text}'. Response: {body}"
                )
            }
        }
    }

    fn consolidate_cache(&self) {
        // Collect individual cache files to delete after consolidation
        let mut files_to_delete = Vec::new();

        // Scan the cache directory for individual cache files and merge them
        if let Ok(entries) = std::fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Skip if it's the master cache file or not a JSON file
                if path == self.master_cache_file
                    || path.extension().and_then(|s| s.to_str()) != Some("json")
                {
                    continue;
                }

                // Extract hash from filename
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str())
                    && let Ok(hash) = filename.parse::<u64>()
                {
                    // Read the translation from the file
                    if let Ok(translation) = std::fs::read_to_string(&path) {
                        // Add to consolidated cache if not already present
                        self.cache.entry(hash).or_insert_with(|| translation);
                        // Mark this file for deletion
                        files_to_delete.push(path);
                    }
                }
            }
        }

        // Skip rewriting the master cache if nothing changed
        if files_to_delete.is_empty() {
            return;
        }

        // Write the consolidated cache to the master file
        // Convert to BTreeMap for deterministic serialization order
        let sorted_cache: std::collections::BTreeMap<_, _> = self
            .cache
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();

        if let Ok(json) = serde_json::to_string_pretty(&sorted_cache)
            && std::fs::write(&self.master_cache_file, json).is_ok()
        {
            // Only delete individual files if the master cache was written successfully
            for file in files_to_delete {
                let _ = std::fs::remove_file(file);
            }
        }
    }
}

impl Drop for GoogleTranslator {
    fn drop(&mut self) {
        self.consolidate_cache();
    }
}
