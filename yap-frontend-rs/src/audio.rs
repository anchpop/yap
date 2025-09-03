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
        let cache_text = format!(
            "{provider:?}:{text}:{language}",
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
                    return Some(cached_bytes);
                }
                Err(_) => {
                    // File exists but couldn't read
                }
            }
        }
        None
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
        {
            if let Ok(mut writable) = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await
            {
                let _ = writable.write_at_cursor_pos(bytes).await;
                let _ = writable.close().await;
            }
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
        };

        let response = hit_ai_server(endpoint, request, access_token)
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
