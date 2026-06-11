//! Small wrapper around Google Cloud Text-to-Speech with retry-on-defect.
//!
//! Google's TTS occasionally returns silent or truncated audio for short
//! utterances. This crate wraps the synthesize call with a retry loop and
//! reports whether the final audio passed defect checks or whether all
//! attempts were defective and we returned the last one anyway.
//!
//! The crate is application-agnostic — callers map their own language enums
//! into the `language_code` + `voice_name` strings the Google API takes.

use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct GoogleTtsRequest {
    pub text: String,
    /// BCP-47 language tag, e.g. `"fr-FR"`.
    pub language_code: String,
    /// Google TTS voice name, e.g. `"fr-FR-Chirp3-HD-Achernar"`.
    pub voice_name: String,
    /// Playback speed multiplier (1.0 = normal).
    pub speed: f64,
    /// Treat `text` as SSML rather than plain text.
    pub is_ssml: bool,
}

/// What we got back from the API after the retry loop, plus a status
/// indicating whether defect checks passed. We *always* return audio bytes if
/// the API call succeeded — `status` lets the caller decide what to do when
/// every attempt was flagged as defective.
#[derive(Debug, Clone)]
pub struct GoogleTtsOutcome {
    /// OGG/Opus-encoded audio bytes (the encoding we always request).
    pub audio_bytes: Vec<u8>,
    /// Total attempts made, including the one whose audio we returned.
    pub attempts: usize,
    /// `Passed` if the returned audio passed defect checks. `HitLimit` if
    /// every attempt was flagged and we returned the last one regardless.
    pub status: TtsStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsStatus {
    /// Returned audio passed `audio_defect` after `attempts` tries.
    Passed,
    /// Hit the retry limit; the returned audio is whatever the last attempt
    /// produced, and `last_defect` is what flagged it.
    HitLimit { last_defect: &'static str },
}

impl TtsStatus {
    pub fn passed(&self) -> bool {
        matches!(self, TtsStatus::Passed)
    }
}

#[derive(Debug, Clone)]
pub struct GoogleTtsClient {
    api_key: String,
    http: reqwest::Client,
    max_attempts: usize,
}

impl GoogleTtsClient {
    pub fn new(api_key: String) -> Self {
        Self::with_http(api_key, reqwest::Client::new())
    }

    pub fn with_http(api_key: String, http: reqwest::Client) -> Self {
        Self {
            api_key,
            http,
            max_attempts: 5,
        }
    }

    /// Override the retry budget. Defaults to 5.
    pub fn with_max_attempts(mut self, n: usize) -> Self {
        self.max_attempts = n.max(1);
        self
    }

    /// Call Google TTS with the retry-on-defect loop. Returns once we either
    /// get audio that passes [`audio_defect`] or exhaust `max_attempts`.
    pub async fn synthesize(&self, request: &GoogleTtsRequest) -> Result<GoogleTtsOutcome> {
        let url = format!(
            "https://texttospeech.googleapis.com/v1beta1/text:synthesize?key={}",
            self.api_key
        );

        let input = if request.is_ssml {
            GoogleTtsInput {
                text: None,
                ssml: Some(request.text.clone()),
            }
        } else {
            GoogleTtsInput {
                text: Some(request.text.clone()),
                ssml: None,
            }
        };
        let payload = GoogleTtsRequestBody {
            input,
            voice: GoogleTtsVoice {
                language_code: request.language_code.clone(),
                name: request.voice_name.clone(),
            },
            audio_config: GoogleTtsAudioConfig {
                audio_encoding: "OGG_OPUS".to_string(),
                speaking_rate: request.speed,
            },
        };

        let mut last_bytes: Vec<u8> = Vec::new();
        let mut last_defect: Option<&'static str> = None;
        let mut attempts = 0usize;

        while attempts < self.max_attempts {
            attempts += 1;
            let response = self
                .http
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
                .context("Google TTS request failed")?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("Google TTS error ({status}): {body}");
            }
            let body: GoogleTtsResponseBody = response
                .json()
                .await
                .context("Failed to parse Google TTS response JSON")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&body.audio_content)
                .context("Google TTS audio_content was not valid base64")?;

            match audio_defect(&bytes) {
                None => {
                    return Ok(GoogleTtsOutcome {
                        audio_bytes: bytes,
                        attempts,
                        status: TtsStatus::Passed,
                    });
                }
                Some(defect) => {
                    log::warn!(
                        "google-tts: defective audio ({defect}) on attempt {attempts}/{}, \
                         retrying",
                        self.max_attempts
                    );
                    last_defect = Some(defect);
                    last_bytes = bytes;
                }
            }
        }

        Ok(GoogleTtsOutcome {
            audio_bytes: last_bytes,
            attempts,
            status: TtsStatus::HitLimit {
                last_defect: last_defect.unwrap_or("unknown"),
            },
        })
    }
}

// --- request/response wire types (private — exposed via GoogleTtsRequest) ---

#[derive(Serialize)]
struct GoogleTtsRequestBody {
    input: GoogleTtsInput,
    voice: GoogleTtsVoice,
    #[serde(rename = "audioConfig")]
    audio_config: GoogleTtsAudioConfig,
}

#[derive(Serialize)]
struct GoogleTtsInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssml: Option<String>,
}

#[derive(Serialize)]
struct GoogleTtsVoice {
    #[serde(rename = "languageCode")]
    language_code: String,
    name: String,
}

#[derive(Serialize)]
struct GoogleTtsAudioConfig {
    #[serde(rename = "audioEncoding")]
    audio_encoding: String,
    #[serde(rename = "speakingRate")]
    speaking_rate: f64,
}

#[derive(Deserialize)]
struct GoogleTtsResponseBody {
    #[serde(rename = "audioContent")]
    audio_content: String,
}

// --- audio defect detection -------------------------------------------------

/// Returns `Some(reason)` if the audio looks defective enough to warrant a
/// retry, or `None` if it's acceptable. Only short clips (<1s) are inspected
/// — longer outputs are assumed fine, since the failure modes we care about
/// (silence, truncation) show up in short utterances.
pub fn audio_defect(audio_bytes: &[u8]) -> Option<&'static str> {
    let (samples, sample_rate) = match decode_audio_to_f32(audio_bytes) {
        Ok((s, sr)) if !s.is_empty() && sr > 0 => (s, sr),
        _ => return Some("failed to decode"),
    };
    samples_defect(&samples, sample_rate)
}

/// Same checks as [`audio_defect`] but on pre-decoded mono f32 samples.
/// Use this when the caller already has the audio in PCM form (e.g.
/// decoded via ffmpeg for WAV input).
pub fn samples_defect(samples: &[f32], sample_rate: u32) -> Option<&'static str> {
    if samples.is_empty() || sample_rate == 0 {
        return Some("failed to decode");
    }

    let duration_s = samples.len() as f64 / sample_rate as f64;
    if duration_s >= 1.0 {
        return None;
    }

    // Silence check: real speech runs -20 to -10 dB RMS; broken Google TTS
    // sits near -50 dB. 0.01 (-40 dB) sits cleanly between the two, and
    // using RMS (not peak) keeps us robust to isolated clicks in
    // otherwise-silent output.
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
    if rms < 0.01 {
        return Some("silent");
    }

    // Tail-decay check for very short clips: a full utterance fades 50-70 dB
    // from its peak to the end of the clip; a truncated one barely drops.
    if duration_s < 0.5 && tail_decay_db(samples, sample_rate).is_some_and(|d| d < 15.0) {
        return Some("cut off");
    }

    None
}

/// Estimates how many dB the signal drops between its loudest frame and the
/// end of the clip, using the slope of the last 100 ms of the RMS envelope.
/// Returns `None` if the clip is too short to build a meaningful envelope.
fn tail_decay_db(samples: &[f32], sample_rate: u32) -> Option<f32> {
    const HOP_S: f32 = 0.010;
    const WIN_S: f32 = 0.020;
    const TAIL_FRAMES: usize = 10; // 100 ms at 10 ms hop

    let win = (sample_rate as f32 * WIN_S) as usize;
    let hop = (sample_rate as f32 * HOP_S) as usize;
    if win == 0 || hop == 0 || samples.len() < win {
        return None;
    }

    let mut envelope = Vec::new();
    let mut start = 0;
    while start + win <= samples.len() {
        let slice = &samples[start..start + win];
        let sum_sq: f64 = slice.iter().map(|&s| (s as f64) * (s as f64)).sum();
        envelope.push((sum_sq / slice.len() as f64).sqrt() as f32);
        start += hop;
    }
    if envelope.len() < TAIL_FRAMES {
        return None;
    }

    let tail_db: Vec<f32> = envelope[envelope.len() - TAIL_FRAMES..]
        .iter()
        .map(|&r| 20.0 * r.max(1e-6).log10())
        .collect();
    let n = tail_db.len() as f32;
    let mean_x = (n - 1.0) / 2.0 * HOP_S;
    let mean_y = tail_db.iter().sum::<f32>() / n;
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for (i, &y) in tail_db.iter().enumerate() {
        let x = i as f32 * HOP_S;
        num += (x - mean_x) * (y - mean_y);
        den += (x - mean_x).powi(2);
    }
    if den == 0.0 {
        return None;
    }
    let slope_db_per_s = num / den;

    let peak_idx = envelope
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?
        .0;
    let peak_to_end_s = (envelope.len() - 1 - peak_idx) as f32 * HOP_S;
    Some(peak_to_end_s * slope_db_per_s.abs())
}

// --- audio decoders ---------------------------------------------------------

/// Dispatches to the right decoder based on magic bytes. Handles the formats
/// we actually see from our TTS providers and pipelines: OGG Opus (Google),
/// WAV (Gemini), and MP3 (OpenAI and others).
pub fn decode_audio_to_f32(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if bytes.starts_with(b"OggS") {
        decode_ogg_opus_to_f32(bytes)
    } else if bytes.starts_with(b"RIFF") {
        decode_wav_to_f32(bytes)
    } else {
        decode_mp3_to_f32(bytes)
    }
}

/// Decodes WAV (any bit depth hound supports) to mono f32 samples.
pub fn decode_wav_to_f32(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    use std::io::Cursor;

    let mut reader =
        hound::WavReader::new(Cursor::new(bytes)).map_err(|e| format!("wav read error: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| format!("wav decode error: {e}"))?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|s| s as f32 / max))
                .collect::<Result<_, _>>()
                .map_err(|e| format!("wav decode error: {e}"))?
        }
    };

    let samples: Vec<f32> = interleaved
        .chunks(channels)
        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
        .collect();

    if samples.is_empty() {
        return Err("No audio data decoded".to_string());
    }

    Ok((samples, spec.sample_rate))
}

pub fn decode_mp3_to_f32(mp3_bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    use std::io::Cursor;

    let cursor = Cursor::new(mp3_bytes);
    let mut decoder = minimp3::Decoder::new(cursor);

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_rate = 0u32;

    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                sample_rate = frame.sample_rate as u32;
                let channels = frame.channels;
                for chunk in frame.data.chunks(channels) {
                    let mono = chunk.iter().map(|&s| s as f32).sum::<f32>() / channels as f32;
                    samples.push(mono / 32768.0);
                }
            }
            Err(minimp3::Error::Eof) => break,
            Err(e) => return Err(format!("mp3 decode error: {e:?}")),
        }
    }

    if samples.is_empty() {
        return Err("No audio data decoded".to_string());
    }

    Ok((samples, sample_rate))
}

pub fn decode_ogg_opus_to_f32(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    use std::io::Cursor;

    let mut reader = ogg::PacketReader::new(Cursor::new(bytes));

    // First packet: OpusHead identification header (RFC 7845 §5.1).
    let header = reader
        .read_packet_expected()
        .map_err(|e| format!("ogg read error: {e:?}"))?;
    if !header.data.starts_with(b"OpusHead") || header.data.len() < 19 {
        return Err("not an OpusHead packet".to_string());
    }
    let channels = header.data[9];
    let opus_channels = match channels {
        1 => opus::Channels::Mono,
        2 => opus::Channels::Stereo,
        n => return Err(format!("unsupported channel count: {n}")),
    };

    // Second packet: OpusTags comment header — discard.
    let _tags = reader
        .read_packet_expected()
        .map_err(|e| format!("ogg tags read error: {e:?}"))?;

    // Opus always decodes at 48 kHz internally; pick that as our output rate.
    const DECODE_RATE: u32 = 48_000;
    let mut decoder = opus::Decoder::new(DECODE_RATE, opus_channels)
        .map_err(|e| format!("opus decoder init: {e:?}"))?;

    // Max Opus frame is 120 ms at 48 kHz = 5760 samples per channel.
    let mut frame_buf = vec![0f32; 5760 * channels as usize];
    let mut samples: Vec<f32> = Vec::new();

    while let Some(packet) = reader
        .read_packet()
        .map_err(|e| format!("ogg packet read: {e:?}"))?
    {
        let n = decoder
            .decode_float(&packet.data, &mut frame_buf, false)
            .map_err(|e| format!("opus decode: {e:?}"))?;
        for chunk in frame_buf[..n * channels as usize].chunks(channels as usize) {
            let mono = chunk.iter().sum::<f32>() / channels as f32;
            samples.push(mono);
        }
    }

    if samples.is_empty() {
        return Err("No audio data decoded".to_string());
    }

    Ok((samples, DECODE_RATE))
}
