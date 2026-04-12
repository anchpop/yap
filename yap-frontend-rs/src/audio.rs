use crate::{AudioRequest, TtsRequest, persistent, utils::hit_ai_server};
use base64::Engine;
use language_utils::TtsProvider;
use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};
use std::collections::BTreeSet;
use wasm_bindgen::JsValue;
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;

#[derive(Clone)]
pub struct AudioCache {
    audio_dir: opfs::persistent::DirectoryHandle,
}

impl AudioCache {
    pub async fn new() -> Result<Self, JsValue> {
        let root = persistent::app_specific_dir()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to get app directory: {e:?}")))?;

        let audio_dir = root
            .get_directory_handle_with_options(
                "audio",
                &opfs::GetDirectoryHandleOptions { create: true },
            )
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to get audio directory: {e:?}")))?;

        Ok(Self { audio_dir })
    }

    pub fn get_cache_filename(request: &TtsRequest, provider: &TtsProvider) -> String {
        let instructions = request.instructions.as_deref().unwrap_or("");
        let cache_text = format!(
            "{provider:?}:{text}:{language}:{instructions}",
            text = request.text,
            language = request.language
        );
        let cache_key = const_xxh3(cache_text.as_bytes());
        format!("{cache_key}.mp3")
    }

    pub async fn get_cached(
        &self,
        request: &TtsRequest,
        provider: &TtsProvider,
    ) -> Option<Vec<u8>> {
        let cache_filename = Self::get_cache_filename(request, provider);

        if let Ok(file_handle) = self
            .audio_dir
            .get_file_handle_with_options(
                &cache_filename,
                &opfs::GetFileHandleOptions { create: false },
            )
            .await
        {
            match file_handle.read().await {
                Ok(cached_bytes) => {
                    if is_valid_audio_data(&cached_bytes) {
                        return Some(cached_bytes);
                    }

                    log::warn!("Invalid audio cache detected for {cache_filename}, refetching");
                    let mut audio_dir = self.audio_dir.clone();
                    if let Err(e) = audio_dir.remove_entry(&cache_filename).await {
                        log::warn!("Failed to remove invalid audio cache {cache_filename}: {e:?}");
                    }
                }
                Err(_) => {
                    // File exists but couldn't read
                    let mut audio_dir = self.audio_dir.clone();
                    if let Err(e) = audio_dir.remove_entry(&cache_filename).await {
                        log::warn!(
                            "Failed to remove unreadable audio cache {cache_filename}: {e:?}"
                        );
                    }
                }
            }
        }
        None
    }

    pub async fn remove_cached(
        &self,
        request: &TtsRequest,
        provider: &TtsProvider,
    ) -> Result<(), JsValue> {
        let cache_filename = Self::get_cache_filename(request, provider);

        let mut audio_dir = self.audio_dir.clone();
        if let Err(e) = audio_dir.remove_entry(&cache_filename).await {
            log::warn!("Failed to remove audio cache {cache_filename}: {e:?}");
        }

        Ok(())
    }

    pub async fn cache_audio(&self, request: &TtsRequest, provider: &TtsProvider, bytes: Vec<u8>) {
        let cache_filename = Self::get_cache_filename(request, provider);

        if let Ok(mut file_handle) = self
            .audio_dir
            .get_file_handle_with_options(
                &cache_filename,
                &opfs::GetFileHandleOptions { create: true },
            )
            .await
            && let Ok(mut writable) = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await
        {
            let _ = writable.write_at_cursor_pos(&bytes).await;
            let _ = writable.close().await;
        }
    }

    pub async fn fetch_and_cache(
        &self,
        request: &AudioRequest,
        access_token: Option<&String>,
    ) -> Result<Vec<u8>, JsValue> {
        let AudioRequest { request, provider } = request;

        // Check cache first
        if let Some(cached_bytes) = self.get_cached(request, provider).await {
            return Ok(cached_bytes);
        }

        let endpoint = match provider {
            TtsProvider::Google => "/tts/google",
            TtsProvider::ElevenLabs => "/tts",
            TtsProvider::OpenAI => "/tts/openai",
            TtsProvider::Gemini => "/tts/gemini",
        };

        let response = hit_ai_server(
            fetch_happen::Method::POST,
            endpoint,
            Some(request),
            access_token,
        )
        .await
        .map_err(|e| JsValue::from_str(&format!("Request error: {e:?}")))?;

        if !response.ok() {
            return Err(JsValue::from_str(&format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let audio_data = response
            .text()
            .await
            .map_err(|e| JsValue::from_str(&format!("Response parsing error: {e:?}")))?;

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&audio_data)
            .map_err(|e| JsValue::from_str(&format!("Base64 decode error: {e:?}")))?;

        // Cache the audio data
        self.cache_audio(request, provider, bytes.clone()).await;

        Ok(bytes)
    }

    pub async fn cleanup_except(
        &mut self,
        keep_filenames: BTreeSet<String>,
    ) -> Result<(), JsValue> {
        use futures::StreamExt;

        // First, collect all files to delete
        let files_to_delete = {
            let mut entries = self.audio_dir.entries().await.map_err(|e| {
                JsValue::from_str(&format!("Failed to read audio directory: {e:?}"))
            })?;

            let mut files = Vec::new();

            while let Some(Ok((filename, _))) = entries.next().await {
                if filename.ends_with(".mp3") && !keep_filenames.contains(&filename) {
                    files.push(filename);
                }
            }

            files
        };

        // Delete the files
        for filename in files_to_delete {
            log::info!("Removing unused audio file: {filename}");
            if let Err(e) = self.audio_dir.remove_entry(&filename).await {
                log::info!("Failed to remove audio file {filename}: {e:?}");
            }
        }

        Ok(())
    }
}

/// A temporary audio cache that stores files with timestamps in the filename.
/// Files older than 24 hours are cleaned up automatically.
pub struct TempAudioCache {
    temp_dir: opfs::persistent::DirectoryHandle,
}

impl TempAudioCache {
    pub async fn new() -> Result<Self, JsValue> {
        let root = persistent::app_specific_dir()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to get app directory: {e:?}")))?;

        let temp_dir = root
            .get_directory_handle_with_options(
                "audio_temp",
                &opfs::GetDirectoryHandleOptions { create: true },
            )
            .await
            .map_err(|e| {
                JsValue::from_str(&format!("Failed to get temp audio directory: {e:?}"))
            })?;

        Ok(Self { temp_dir })
    }

    fn get_cache_filename(request: &TtsRequest, provider: &TtsProvider) -> String {
        let instructions = request.instructions.as_deref().unwrap_or("");
        let cache_text = format!(
            "{provider:?}:{text}:{language}:{instructions}",
            text = request.text,
            language = request.language
        );
        let cache_key = const_xxh3(cache_text.as_bytes());
        format!("{cache_key}.mp3")
    }

    /// Filename format: `{timestamp_secs}_{hash}.mp3`
    fn temp_filename(base_filename: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        format!("{now}_{base_filename}")
    }

    /// Extract timestamp from a temp filename, returns None if the format doesn't match.
    fn parse_timestamp(filename: &str) -> Option<i64> {
        let (ts, _) = filename.split_once('_')?;
        ts.parse().ok()
    }

    /// Extract the base filename (hash part) from a temp filename.
    fn parse_base_filename(filename: &str) -> Option<&str> {
        let (_, base) = filename.split_once('_')?;
        Some(base)
    }

    /// Find an existing cached file by its base filename (hash), regardless of timestamp.
    async fn find_cached(&self, base_filename: &str) -> Option<(String, Vec<u8>)> {
        use futures::StreamExt;

        let mut entries = self.temp_dir.entries().await.ok()?;
        while let Some(Ok((filename, _))) = entries.next().await {
            if Self::parse_base_filename(&filename) == Some(base_filename) {
                if let Ok(file_handle) = self
                    .temp_dir
                    .get_file_handle_with_options(
                        &filename,
                        &opfs::GetFileHandleOptions { create: false },
                    )
                    .await
                {
                    if let Ok(bytes) = file_handle.read().await {
                        if is_valid_audio_data(&bytes) {
                            return Some((filename, bytes));
                        }
                    }
                }
            }
        }
        None
    }

    pub async fn fetch_and_cache(
        &self,
        request: &AudioRequest,
        access_token: Option<&String>,
    ) -> Result<Vec<u8>, JsValue> {
        let AudioRequest { request, provider } = request;
        let base_filename = Self::get_cache_filename(request, provider);

        // Check cache first (match by hash, ignore timestamp)
        if let Some((_filename, bytes)) = self.find_cached(&base_filename).await {
            return Ok(bytes);
        }

        let endpoint = match provider {
            TtsProvider::Google => "/tts/google",
            TtsProvider::ElevenLabs => "/tts",
            TtsProvider::OpenAI => "/tts/openai",
            TtsProvider::Gemini => "/tts/gemini",
        };

        let response = hit_ai_server(
            fetch_happen::Method::POST,
            endpoint,
            Some(request),
            access_token,
        )
        .await
        .map_err(|e| JsValue::from_str(&format!("Request error: {e:?}")))?;

        if !response.ok() {
            return Err(JsValue::from_str(&format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let audio_data = response
            .text()
            .await
            .map_err(|e| JsValue::from_str(&format!("Response parsing error: {e:?}")))?;

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&audio_data)
            .map_err(|e| JsValue::from_str(&format!("Base64 decode error: {e:?}")))?;

        // Cache with timestamp in filename
        let temp_filename = Self::temp_filename(&base_filename);
        if let Ok(mut file_handle) = self
            .temp_dir
            .get_file_handle_with_options(
                &temp_filename,
                &opfs::GetFileHandleOptions { create: true },
            )
            .await
            && let Ok(mut writable) = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await
        {
            let _ = writable.write_at_cursor_pos(&bytes).await;
            let _ = writable.close().await;
        }

        Ok(bytes)
    }

    /// Remove all temp audio files older than 24 hours.
    pub async fn cleanup_old(&mut self) -> Result<(), JsValue> {
        use futures::StreamExt;

        let cutoff = chrono::Utc::now().timestamp() - 24 * 60 * 60;

        let files_to_delete = {
            let mut entries = self.temp_dir.entries().await.map_err(|e| {
                JsValue::from_str(&format!("Failed to read temp audio directory: {e:?}"))
            })?;

            let mut files = Vec::new();
            while let Some(Ok((filename, _))) = entries.next().await {
                if let Some(ts) = Self::parse_timestamp(&filename) {
                    if ts < cutoff {
                        files.push(filename);
                    }
                }
            }
            files
        };

        for filename in files_to_delete {
            log::info!("Removing expired temp audio file: {filename}");
            if let Err(e) = self.temp_dir.remove_entry(&filename).await {
                log::info!("Failed to remove temp audio file {filename}: {e:?}");
            }
        }

        Ok(())
    }
}

fn is_valid_audio_data(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }

    // MP3: ID3 tag or MPEG frame sync (0xFFF)
    // WAV: RIFF header (Gemini TTS returns WAV)
    bytes.starts_with(b"ID3")
        || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0)
        || bytes.starts_with(b"RIFF")
}
