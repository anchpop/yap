//! Does the extracted track really carry the film's own dialogue, in the
//! language the library says the film is in?
//!
//! Stream tags answer this only when the disc bothered: a director's
//! commentary comes first on plenty of rips with no `comment` disposition,
//! and Hong Kong discs label a Cantonese track "chi" exactly as they label
//! the Mandarin dub. Both slipped through in 2026-09-03 — The Mermaid was
//! transcribed from a commentary track, two films from Cantonese ones — and
//! the only symptom was clips that agreed with nothing. So before any
//! transcription is bought on a track, a model listens to a few windows of
//! it and says what it hears. A rejected track is evicted and recorded in
//! `audio-rejected.json`, and the extractor moves on to the next candidate
//! stream on the disc.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};

pub const MODEL: &str = "gemini-3.1-pro-preview";

/// Where in the runtime each sample starts, as a fraction: past the opening
/// credits, short of the closing ones, spread so a single foreign-language
/// scene cannot carry the verdict.
pub const SAMPLE_POINTS: &[f64] = &[0.2, 0.45, 0.7];
/// A second set, for when the first three windows land on fights, music
/// or silence: an action film can go minutes without a line. Silence is
/// never grounds for rejection — after this set the track is accepted
/// unheard.
pub const RETRY_POINTS: &[f64] = &[0.3, 0.55, 0.8];
const SAMPLE_SECS: f64 = 40.0;

/// What the model heard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    /// The language the dialogue is mainly in, with the variety named.
    pub spoken_language: String,
    /// Whether that is the language the library expected.
    pub expected_language_spoken: bool,
    /// Whether the samples carried enough dialogue to judge the track by —
    /// a stray word in a fight scene is not evidence of anything.
    pub enough_dialogue: bool,
    /// People talking about the film over its soundtrack, not the film.
    pub commentary: bool,
    /// "high", "medium" or "low".
    pub confidence: String,
    pub notes: String,
}

impl Verdict {
    /// Only a track the model heard as the film's own dialogue, in the
    /// expected language, may feed the pipeline. A track that gave the
    /// listener too little to go on is not evidence against it.
    pub fn accepted(&self) -> bool {
        !self.commentary && (self.expected_language_spoken || !self.enough_dialogue)
    }
}

/// The language name a listener should judge the track against. Radarr says
/// "Chinese" for every Chinese-language film; the course is Mandarin.
pub fn expected_language(original_language: &str) -> &str {
    match original_language {
        "Chinese" | "Mandarin" => "Mandarin Chinese",
        other => other,
    }
}

/// A window of the track at each of `points` (fractions of the runtime),
/// as mono 16 kHz opus for the wire.
///
/// Reads the extracted opus, so this costs a few seeks — never a pass over
/// the remux.
pub fn samples(audio: &Path, duration_ms: i64, points: &[f64]) -> Result<Vec<Vec<u8>>> {
    let runtime = duration_ms as f64 / 1000.0;
    if runtime < SAMPLE_SECS * 2.0 {
        bail!("track is only {runtime:.0}s long");
    }
    points
        .iter()
        .map(|point| {
            let start = (runtime * point).min(runtime - SAMPLE_SECS);
            let out = Command::new("ffmpeg")
                .args(["-v", "error", "-ss", &format!("{start:.3}")])
                .args(["-t", &format!("{SAMPLE_SECS:.3}"), "-i"])
                .arg(audio)
                .args([
                    "-ac", "1", "-ar", "16000", "-c:a", "libopus", "-b:a", "24k", "-f", "ogg", "-",
                ])
                .output()
                .context("ffmpeg failed to start")?;
            if !out.status.success() || out.stdout.is_empty() {
                bail!("ffmpeg could not cut the sample at {start:.0}s");
            }
            Ok(out.stdout)
        })
        .collect()
}

fn prompt(expected: &str) -> String {
    format!(
        "You are listening to three samples from one audio track of a feature film, taken at \
         roughly a quarter, half and three quarters of the way through. The library lists the \
         film's language as {expected}, and this track was picked as the film's own \
         original-language dialogue track. Judge whether it really is.\n\n\
         spoken_language: the language the dialogue is mainly in. Name the variety where it \
         matters — Mandarin against Cantonese, European against Brazilian Portuguese — since a \
         course in one variety cannot use the other.\n\
         expected_language_spoken: whether the dialogue is mainly in {expected}. A film may \
         switch languages for a scene, so answer for the track as a whole, not for any one \
         line.\n\
         enough_dialogue: whether the samples carried enough dialogue to judge the track \
         by — several full lines, not a stray word or two in a fight or a chase. When there \
         was not, say so here and leave expected_language_spoken false rather than guessing; \
         a track is never rejected for being quiet, only for what was clearly heard.\n\
         commentary: whether this is a commentary track — a director, cast or critics talking \
         about the film over its soundtrack, with the film's own dialogue faint or absent \
         underneath — rather than the film itself.\n\
         confidence: high, medium or low.\n\
         notes: a sentence or two on what you heard that decided it.\n\n\
         Music and silence are fine; a sample with no speech says nothing either way, so lean \
         on the ones that have some."
    )
}

/// Ask the model what it hears on the track.
pub async fn judge(
    http: &reqwest::Client,
    key: &str,
    expected: &str,
    samples: &[Vec<u8>],
) -> Result<Verdict> {
    let mut parts = vec![serde_json::json!({ "text": prompt(expected) })];
    for sample in samples {
        parts.push(serde_json::json!({
            "inlineData": {
                "mimeType": "audio/ogg",
                "data": base64::engine::general_purpose::STANDARD.encode(sample),
            }
        }));
    }
    let body = serde_json::json!({
        "contents": [{ "parts": parts }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": {
                "type": "OBJECT",
                "properties": {
                    "spoken_language": { "type": "STRING" },
                    "expected_language_spoken": { "type": "BOOLEAN" },
                    "enough_dialogue": { "type": "BOOLEAN" },
                    "commentary": { "type": "BOOLEAN" },
                    "confidence": { "type": "STRING", "enum": ["high", "medium", "low"] },
                    "notes": { "type": "STRING" },
                },
                "required": ["spoken_language", "expected_language_spoken", "enough_dialogue", "commentary", "confidence", "notes"],
            },
        },
    });
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:generateContent");

    let mut last = None;
    for attempt in 0..4 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(5 << attempt)).await;
        }
        let response = http
            .post(&url)
            .header("x-goog-api-key", key)
            .json(&body)
            .send()
            .await;
        let response = match response {
            Ok(r) => r,
            Err(e) => {
                last = Some(anyhow::Error::from(e));
                continue;
            }
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        // Overload and rate limits are worth a retry; anything else is ours.
        if status.as_u16() == 429 || status.is_server_error() {
            last = Some(anyhow::anyhow!(
                "{status}: {}",
                text.chars().take(200).collect::<String>()
            ));
            continue;
        }
        if !status.is_success() {
            bail!("{status}: {}", text.chars().take(500).collect::<String>());
        }
        return parse(&text);
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("no response")))
}

fn parse(text: &str) -> Result<Verdict> {
    #[derive(Deserialize)]
    struct Response {
        candidates: Vec<Candidate>,
    }
    #[derive(Deserialize)]
    struct Candidate {
        content: Content,
    }
    #[derive(Deserialize)]
    struct Content {
        parts: Vec<Part>,
    }
    #[derive(Deserialize)]
    struct Part {
        text: String,
    }
    let response: Response = serde_json::from_str(text).context("unexpected response shape")?;
    let answer = response
        .candidates
        .into_iter()
        .next()
        .and_then(|c| c.content.parts.into_iter().next())
        .context("response has no candidate")?
        .text;
    serde_json::from_str(&answer).with_context(|| format!("verdict is not the schema: {answer}"))
}
