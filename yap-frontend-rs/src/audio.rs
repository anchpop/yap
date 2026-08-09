use crate::{AudioRequest, TtsRequest, human_audio, persistent, utils::hit_ai_server};
use base64::Engine;
use language_utils::{Compensation, TtsProvider};
use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};
use std::collections::BTreeSet;
use wasm_bindgen::JsValue;
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;

/// Result of fetching an audio clip — the bytes plus a sidecar saying who
/// recorded it, when the clip came from a human voice actor (vs. TTS).
pub struct FetchedAudio {
    pub bytes: Vec<u8>,
    pub voice_actor: Option<VoiceActorInfo>,
}

/// Identifies the voice actor behind a human-recorded clip. Crosses the
/// wasm boundary as a plain object (`{ name, compensation }`) so the
/// frontend shares this type rather than redeclaring it.
#[derive(Clone, serde::Serialize, schemars::JsonSchema, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct VoiceActorInfo {
    pub name: String,
    pub compensation: Compensation,
}

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

    pub async fn get_or_evict_cached(
        &self,
        request: &TtsRequest,
        provider: &TtsProvider,
    ) -> Option<Vec<u8>> {
        let cache_filename = tts_cache_filename(request, provider);

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

                    let head: Vec<u8> = cached_bytes.iter().take(8).copied().collect();
                    log::warn!(
                        "Invalid audio cache detected for {cache_filename} ({} bytes, magic={head:02x?}), refetching",
                        cached_bytes.len()
                    );
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
        let cache_filename = tts_cache_filename(request, provider);

        let mut audio_dir = self.audio_dir.clone();
        if let Err(e) = audio_dir.remove_entry(&cache_filename).await {
            log::warn!("Failed to remove audio cache {cache_filename}: {e:?}");
        }

        Ok(())
    }

    pub async fn cache_audio(&self, request: &TtsRequest, provider: &TtsProvider, bytes: Vec<u8>) {
        let cache_filename = tts_cache_filename(request, provider);

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
    ) -> Result<FetchedAudio, JsValue> {
        let AudioRequest { request, provider } = request;

        // Human recordings live in the language pack and don't need OPFS caching.
        // Only serve them for a plain request — see `human_audio_applies`.
        if human_audio_applies(request)
            && let Some(human) = human_audio::lookup(request.language, &request.text)
        {
            return Ok(FetchedAudio {
                bytes: human.bytes,
                voice_actor: Some(VoiceActorInfo {
                    name: human.actor_name,
                    compensation: human.compensation,
                }),
            });
        }

        // Check cache first
        if let Some(cached_bytes) = self.get_or_evict_cached(request, provider).await {
            return Ok(FetchedAudio {
                bytes: cached_bytes,
                voice_actor: None,
            });
        }

        let bytes = fetch_tts(request, provider, access_token)
            .await
            .map_err(|e| JsValue::from_str(&e))?;

        // Cache the audio data
        self.cache_audio(request, provider, bytes.clone()).await;

        Ok(FetchedAudio {
            bytes,
            voice_actor: None,
        })
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
            if Self::parse_base_filename(&filename) == Some(base_filename)
                && let Ok(file_handle) = self
                    .temp_dir
                    .get_file_handle_with_options(
                        &filename,
                        &opfs::GetFileHandleOptions { create: false },
                    )
                    .await
                && let Ok(bytes) = file_handle.read().await
                && is_valid_audio_data(&bytes)
            {
                return Some((filename, bytes));
            }
        }
        None
    }

    pub async fn fetch_and_cache(
        &self,
        request: &AudioRequest,
        access_token: Option<&String>,
    ) -> Result<FetchedAudio, JsValue> {
        let AudioRequest { request, provider } = request;

        // Human recordings live in the language pack and don't need OPFS caching.
        // Only serve them for a plain request — see `human_audio_applies`.
        if human_audio_applies(request)
            && let Some(human) = human_audio::lookup(request.language, &request.text)
        {
            return Ok(FetchedAudio {
                bytes: human.bytes,
                voice_actor: Some(VoiceActorInfo {
                    name: human.actor_name,
                    compensation: human.compensation,
                }),
            });
        }

        let base_filename = tts_cache_filename(request, provider);

        // Check cache first (match by hash, ignore timestamp)
        if let Some((_filename, bytes)) = self.find_cached(&base_filename).await {
            return Ok(FetchedAudio {
                bytes,
                voice_actor: None,
            });
        }

        let bytes = fetch_tts(request, provider, access_token)
            .await
            .map_err(|e| JsValue::from_str(&e))?;

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

        Ok(FetchedAudio {
            bytes,
            voice_actor: None,
        })
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
                if let Some(ts) = Self::parse_timestamp(&filename)
                    && ts < cutoff
                {
                    files.push(filename);
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

/// Bumped whenever a backend change alters the audio produced for a request
/// that is itself unchanged — a different voice, model, or post-processing
/// step. Nothing in a `TtsRequest` describes *how* the backend renders it, so
/// without this a clip cached before such a change is indistinguishable from
/// one made after, and OPFS keeps serving the old one forever.
///
/// A bump costs every user one round of cache misses. That is the whole point:
/// the alternative is a learner who already cached a defective clip never
/// hearing the fix.
///
/// - 1: Chirp3-HD started eating the text next to a `<break>`, so pronunciation
///   cards had cached audio that omitted the very letter they exist to teach.
const TTS_SYNTHESIS_REVISION: u32 = 1;

/// Cache filename for a TTS request. The key must include *every* input that
/// changes the synthesized audio — otherwise a request differing only in, say,
/// `speed` would be served a stale clip rendered at a different speed. Shared
/// by both `AudioCache` and `TempAudioCache` so the two can never drift apart.
///
/// That includes inputs the request doesn't carry: `TTS_SYNTHESIS_REVISION`
/// stands in for the backend's own rendering decisions.
///
/// `verification_hints` is keyed too, which is easy to talk yourself out of —
/// it never reaches a TTS provider, so it can't change a single sample of any
/// one attempt. It does decide which attempt comes back: hints feed the ASR
/// gate, the gate decides whether a clip is accepted or the providers race,
/// and so the same text with and without hints can legitimately return
/// different audio. Keying them means a pack update that tags a new proper
/// noun re-verifies the sentences containing it rather than trusting a clip
/// that was accepted without ever knowing the name.
pub(crate) fn tts_cache_filename(request: &TtsRequest, provider: &TtsProvider) -> String {
    // Distinguish instructions None ('n') from Some("") ('s'): the /tts
    // handlers treat them differently (None = default prompt, Some("") = empty
    // prefix), so they must key to different clips.
    let (itag, instructions) = match request.instructions.as_deref() {
        Some(s) => ('s', s),
        None => ('n', ""),
    };
    // Length-prefix the free-form fields (text, instructions) so two distinct
    // requests can't collide via a colon embedded in the text — e.g. text
    // "a:b" vs text "a" + instructions "b" would otherwise hash identically.
    // The remaining fields have bounded, colon-free Debug/Display/numeric
    // forms, so they're safe to join directly.
    // Hints are joined with a separator that can't appear inside one (they're
    // single words from the pack) and length-prefixed like the other
    // free-form fields, so ["a", "b"] can't collide with ["a b"].
    let hints = request.verification_hints.join("\u{1f}");
    let cache_text = format!(
        "r{TTS_SYNTHESIS_REVISION}|{provider:?}|{language}|{speed}|{is_ssml}\
         |{tlen}:{text}|{itag}{ilen}:{instructions}|{hlen}:{hints}",
        language = request.language,
        speed = request.speed,
        is_ssml = request.is_ssml,
        tlen = request.text.len(),
        text = request.text,
        ilen = instructions.len(),
        hlen = hints.len(),
    );
    let cache_key = const_xxh3(cache_text.as_bytes());
    format!("{cache_key}.mp3")
}

/// Whether a human recording may satisfy this request. A human clip is a
/// single fixed rendering of the phrase, so it can't honor non-default
/// speed, SSML, or style instructions — when any of those is set we skip
/// the human clip and fall through to TTS, which can actually apply them.
/// (Provider is deliberately ignored: human audio is preferred over any
/// TTS provider for a plain request.)
pub fn human_audio_applies(request: &TtsRequest) -> bool {
    !request.is_ssml && request.instructions.is_none() && (request.speed - 1.0).abs() < f64::EPSILON
}

/// Backend route for each TTS provider — shared with the native MCP server,
/// whose transport is reqwest rather than the browser fetch used here.
pub fn tts_endpoint(provider: &TtsProvider) -> &'static str {
    match provider {
        TtsProvider::Google => "/tts/google",
        TtsProvider::ElevenLabs => "/tts",
        TtsProvider::OpenAI => "/tts/openai",
        TtsProvider::Gemini => "/tts/gemini",
    }
}

/// Synthesize audio for a request via the AI backend, returning decoded
/// bytes. Shared with the native MCP server — fetch-happen's native
/// transport makes the browser and native wire calls identical.
pub async fn fetch_tts(
    request: &TtsRequest,
    provider: &TtsProvider,
    access_token: Option<&String>,
) -> Result<Vec<u8>, String> {
    let endpoint = tts_endpoint(provider);

    let response = hit_ai_server(
        fetch_happen::Method::POST,
        endpoint,
        Some(request),
        access_token,
    )
    .await
    .map_err(|e| format!("Request error: {e:?}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let audio_data = response
        .text()
        .await
        .map_err(|e| format!("Response parsing error: {e:?}"))?;

    base64::engine::general_purpose::STANDARD
        .decode(&audio_data)
        .map_err(|e| format!("Base64 decode error: {e:?}"))
}

/// Container format sniffed from magic bytes, as a mime type for playback.
pub fn audio_mime_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"RIFF") {
        "audio/wav"
    } else if bytes.starts_with(b"OggS") {
        "audio/ogg"
    } else {
        "audio/mpeg"
    }
}

fn is_valid_audio_data(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }

    // MP3: ID3 tag or MPEG frame sync (0xFFF)
    // WAV: RIFF header (Gemini TTS returns WAV)
    // Ogg Opus: "OggS" page header (Google TTS returns OGG_OPUS)
    bytes.starts_with(b"ID3")
        || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0)
        || bytes.starts_with(b"RIFF")
        || bytes.starts_with(b"OggS")
}
