use anyhow::Context;
use html_escape::decode_html_entities;
use language_utils::Language;
use std::collections::HashMap;
use std::path::PathBuf;
use xxhash_rust::xxh3::xxh3_64;

pub struct GoogleTranslator {
    client: reqwest::Client,
    source_language: String,
    target_language: String,
    api_key: String,
    cache: HashMap<String, String>,
    cache_dir: PathBuf,
}

impl GoogleTranslator {
    pub fn new(
        source_language: Language,
        target_language: Language,
        cache_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        let api_key = std::env::var("GOOGLE_TRANSLATE_API_KEY")
            .context("GOOGLE_TRANSLATE_API_KEY not set")?;
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            client: reqwest::Client::new(),
            source_language: source_language.iso_639_1().to_string(),
            target_language: target_language.iso_639_1().to_string(),
            api_key,
            cache: HashMap::new(),
            cache_dir,
        })
    }

    pub async fn translate(&mut self, text: &str) -> anyhow::Result<String> {
        if let Some(t) = self.cache.get(text) {
            return Ok(t.clone());
        }

        let hash_input = format!("{}::{}::{text}", self.source_language, self.target_language);
        let hash = xxh3_64(hash_input.as_bytes());
        let cache_file = self.cache_dir.join(format!("{hash}.json"));

        if cache_file.exists() {
            let cached = std::fs::read_to_string(&cache_file)?;
            let cached = decode_html_entities(&cached).to_string();
            self.cache.insert(text.to_string(), cached.clone());
            return Ok(cached);
        }

        let url = format!(
            "https://translation.googleapis.com/language/translate/v2?key={}",
            self.api_key
        );
        let resp = self
            .client
            .post(&url)
            .form(&[
                ("q", text),
                ("source", self.source_language.as_str()),
                ("target", self.target_language.as_str()),
                ("format", "text"),
            ])
            .send()
            .await
            .context("Failed to call Google Translate API")?;
        let value: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse Google Translate response")?;
        let translated = value["data"]["translations"][0]["translatedText"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let translated = decode_html_entities(&translated).to_string();
        self.cache.insert(text.to_string(), translated.clone());
        std::fs::write(&cache_file, &translated)?;
        Ok(translated)
    }
}
