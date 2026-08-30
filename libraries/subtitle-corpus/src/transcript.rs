//! Transcribing a whole film, so a subtitle can be checked against what was
//! said rather than merely against when someone was speaking.
//!
//! Everything else in this crate samples: `sync` and `check` transcribe a
//! handful of 60s windows and fit an offset. That is enough to place a
//! subtitle, and nowhere near enough to judge one. A cue can sit on the right
//! second and still carry a sound tag, a song lyric, a speaker label, or a
//! sentence condensed to half of what the actor actually said. Answering that
//! needs the whole film's words, once, kept.
//!
//! Transcription is ElevenLabs Scribe v2, chosen over Cloudflare and Groq's
//! Whisper endpoints and two Cohere models on twelve blinded reads of two
//! films (English and Japanese) by four independent judges: it placed first in
//! every one. What decided it was fidelity to the things a subtitler removes —
//! it keeps "um", false starts and stammers in English, and Hida dialect forms
//! (見とる, 〜っす, なん) in Japanese where every Whisper flattens them to
//! standard grammar. A transcript that tidies speech the way a subtitler does
//! cannot reveal that the subtitler tidied.
//!
//! Two things make it affordable. The audio is already extracted to
//! `audio.opus`, so nothing decodes a 30GB remux; and the responses are
//! cached in the shared osmo store keyed by the chunk's *decoded samples*, so
//! a re-run after any change upstream of the transcript costs nothing.
//!
//! # Where the cuts go
//!
//! A transcriber hallucinates in silence, and a cut through the middle of a
//! word invents a word on each side of it. So chunk boundaries are chosen from the
//! film's speech profile rather than by the clock: around each 10-minute mark
//! we search ±90s for the quietest half-second and cut there. Measured over
//! 1,679 boundaries in 150 check-confirmed films, that lands under 0.30 —
//! confident silence — 96.3% of the time, under 0.40 in 99.9%, with a worst
//! case of 0.401 against a nominal voice threshold of 0.5.
//!
//! Note the profile must be the *film's*, computed in one pass: earshot's
//! detector is recurrent, carrying 128 floats of minGRU state plus three
//! frames of context, so scoring the cut chunks separately would start each
//! one cold at precisely the moment we care about.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::vad::BUCKET_MS;

/// A human-readable label stamped into each `transcript.jsonl`, so a file
/// someone stumbles across says roughly what made it.
///
/// Deliberately *not* what decides staleness — [`Provenance::settings`] does
/// that, and it is derived, so a changed parameter is noticed whether or not
/// anyone remembers to touch this string.
const PRODUCED_BY: &str = "elevenlabs_scribe_v2__verbatim_diarize_char";

const SCRIBE_URL: &str = "https://api.elevenlabs.io/v1/speech-to-text";
const SCRIBE_MODEL: &str = "scribe_v2";

/// A superset of word timings — same words, same fields, plus per-character
/// spans — and documented as carrying no surcharge. Worth having for Japanese,
/// where word boundaries are an invention of the tokeniser anyway.
const GRANULARITY: &str = "character";
const DIARIZE: bool = true;
/// Deliberately false. Setting it strips "filler words, false starts and
/// non-speech sounds", which is precisely the material a subtitler removes and
/// this transcript exists to detect.
const NO_VERBATIM: bool = false;

/// The ElevenLabs account transcription runs against.
///
/// Read once at the top of a run, for the same reason as
/// [`crate::sync::WhisperAccount`]: reading the environment per chunk turns a
/// missing key into a per-chunk failure, and a film that fails every chunk
/// looks exactly like a film with nothing to say.
#[derive(Clone)]
pub struct ScribeAccount {
    key: String,
}

impl ScribeAccount {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            key: std::env::var("ELEVENLABS_API_KEY").context("ELEVENLABS_API_KEY not set")?,
        })
    }

    fn key(&self) -> &str {
        &self.key
    }
}

/// How long a chunk runs before we start looking for somewhere to cut.
const TARGET_CHUNK_MS: i64 = 600_000;
/// How far either side of that mark a quieter seam is worth taking.
const SEARCH_WINDOW_MS: i64 = 90_000;
/// A chunk shorter than this is not worth a request of its own.
const MIN_CHUNK_MS: i64 = 120_000;
/// Buckets averaged when judging how quiet a candidate seam is: 5 × 96ms.
const SEAM_BUCKETS: usize = 5;

/// Where a film's chunks begin and end, in milliseconds.
///
/// Boundaries are placed at the quietest seam near each target mark rather
/// than at a fixed interval. A minimum always exists, so there is no failure
/// case — a wall-to-wall talky stretch degrades to the least-bad cut instead
/// of refusing to produce one.
pub fn chunk_bounds(profile: &[f32], film_ms: i64) -> Vec<(i64, i64)> {
    let mut bounds = Vec::new();
    let mut at = 0i64;
    while film_ms - at > TARGET_CHUNK_MS + MIN_CHUNK_MS {
        let target = at + TARGET_CHUNK_MS;
        let lo = (((at + MIN_CHUNK_MS).max(target - SEARCH_WINDOW_MS)) / BUCKET_MS) as usize;
        let hi = ((target + SEARCH_WINDOW_MS) / BUCKET_MS)
            .min(profile.len() as i64 - SEAM_BUCKETS as i64)
            .max(lo as i64 + 1) as usize;
        let mut best: Option<(f32, usize)> = None;
        for i in lo..hi {
            let quiet: f32 = profile[i..i + SEAM_BUCKETS].iter().sum::<f32>() / SEAM_BUCKETS as f32;
            if best.is_none_or(|(q, _)| quiet < q) {
                best = Some((quiet, i));
            }
        }
        let cut = match best {
            Some((_, i)) => (i + SEAM_BUCKETS / 2) as i64 * BUCKET_MS,
            None => target,
        };
        bounds.push((at, cut));
        at = cut;
    }
    bounds.push((at, film_ms));
    bounds
}

/// Everything that shapes a film's transcript, other than the film.
///
/// The per-chunk cache key does not need the chunking constants — moving a
/// boundary changes the audio a chunk contains, so its key moves with it. A
/// film-level digest does need them: they decide how many requests there are
/// and where the joins fall, which is part of what the artifact is.
#[derive(Serialize)]
struct Settings<'a> {
    endpoint: &'a str,
    model_id: &'a str,
    language_code: &'a str,
    timestamps_granularity: &'a str,
    diarize: bool,
    no_verbatim: bool,
    target_chunk_ms: i64,
    search_window_ms: i64,
    min_chunk_ms: i64,
    seam_buckets: usize,
}

/// What produced a transcript, written as its first line.
///
/// This exists so the artifact can be checked against the settings in force
/// rather than merely *existing*. A file that exists is not evidence it was
/// made the way we now make them, and re-running blind is not free: every
/// chunk whose audio hash has drifted for any reason is billed again.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub produced_by: String,
    pub language: String,
    /// Digest of [`Settings`]. Derived rather than hand-maintained, for the
    /// same reason as the cache key: a version string nobody bumps is a
    /// transcript silently kept under settings nobody chose.
    pub settings: String,
    /// The audio stream the words were heard in. A remux that reorders its
    /// tracks, or an extraction that used to pick the director's commentary
    /// (Delicatessen, Face/Off, Psycho all did), changes the audio under a
    /// transcript that would otherwise look current. `None` on transcripts
    /// written before this was recorded; those are trusted as they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<crate::sync::AudioStreamIdentity>,
}

impl Provenance {
    /// Would a transcript with this provenance be redone under `current`?
    /// Settings must match exactly; the audio only when both sides know it.
    pub fn is_stale_against(&self, current: &Provenance) -> bool {
        self.produced_by != current.produced_by
            || self.language != current.language
            || self.settings != current.settings
            || matches!((&self.audio, &current.audio), (Some(a), Some(b)) if a != b)
    }
}

/// The provenance a transcript made right now, for this language and from
/// this audio stream, would carry.
pub fn provenance(
    language: &str,
    audio: Option<crate::sync::AudioStreamIdentity>,
) -> Result<Provenance> {
    let settings = Settings {
        endpoint: SCRIBE_URL,
        model_id: SCRIBE_MODEL,
        language_code: language,
        timestamps_granularity: GRANULARITY,
        diarize: DIARIZE,
        no_verbatim: NO_VERBATIM,
        target_chunk_ms: TARGET_CHUNK_MS,
        search_window_ms: SEARCH_WINDOW_MS,
        min_chunk_ms: MIN_CHUNK_MS,
        seam_buckets: SEAM_BUCKETS,
    };
    Ok(Provenance {
        produced_by: PRODUCED_BY.to_string(),
        language: language.to_string(),
        settings: format!("{:016x}", xxh3_64(&serde_json::to_vec(&settings)?)),
        audio,
    })
}

/// One film's words, as written beside its subtitle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub provenance: Provenance,
    pub words: Vec<Spoken>,
}

impl Transcript {
    /// The artifact as it sits on disk: provenance first, then one word a line.
    ///
    /// Reading the first line is enough to know whether the rest is worth
    /// keeping, which is the whole point of putting it there.
    pub fn to_jsonl(&self) -> Result<String> {
        let mut out = serde_json::to_string(&self.provenance)?;
        out.push('\n');
        for word in &self.words {
            out.push_str(&serde_json::to_string(word)?);
            out.push('\n');
        }
        Ok(out)
    }
}

/// The provenance of an existing transcript, or `None` if it has none.
///
/// A file written before provenance existed has a [`Spoken`] on its first line
/// and reads as `None` — correctly, since nothing recorded how it was made.
pub fn stored_provenance(path: &Path) -> Option<Provenance> {
    let file = std::fs::File::open(path).ok()?;
    let mut first = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(file), &mut first).ok()?;
    serde_json::from_str(&first).ok()
}

/// Is this a word, or a noise the transcriber labelled?
///
/// Scribe emits `[laughs]`, `[coughs]` and the like as their own entries. They
/// are the one thing a subtitle's `[SEXYKITTEN CHUCKLES]` can be checked
/// against, so they are kept and marked rather than discarded — but they must
/// never be counted as dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Word,
    AudioEvent,
}

/// One unit of the transcript, timed from the start of the film.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spoken {
    pub text: String,
    pub at_ms: i64,
    pub until_ms: i64,
    pub kind: Kind,
    /// Which voice, within this chunk only. Scribe numbers speakers per
    /// request, so `speaker_0` in one chunk is unrelated to `speaker_0` in the
    /// next; useful for telling two people apart inside a scene, useless as an
    /// identity across a film.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// The model's own confidence in this word. Near 0 is certain; the
    /// worst words in a clean scene sit around -0.5. Lets a disagreement with
    /// the subtitle be weighted by whether the transcriber was sure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprob: Option<f64>,
}

#[derive(Deserialize)]
struct ScribeResponse {
    #[serde(default)]
    words: Vec<ScribeWord>,
}

/// The response's own fields, cached verbatim so a change of opinion about
/// what to keep does not mean paying for the audio again.
#[derive(Serialize, Deserialize)]
struct ScribeWord {
    #[serde(default)]
    text: String,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
    /// `word`, `spacing` or `audio_event`. Spacing entries carry no content
    /// and are dropped; counting them would inflate every word total.
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    speaker_id: Option<String>,
    #[serde(default)]
    logprob: Option<f64>,
}

/// Cut one chunk out of the extracted audio as raw 16kHz mono PCM.
///
/// PCM and not opus, because this is what the cache key is taken from and
/// opus is *not reproducible*: ffmpeg stamps a random Ogg bitstream serial
/// into every stream it writes, so encoding the same audio twice differs at
/// byte 15 and every key would be a miss. Decoded samples are byte-identical
/// run to run, which is what a content-addressed cache needs.
fn slice_pcm(audio: &Path, start_ms: i64, end_ms: i64) -> Result<Vec<u8>> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss"])
        .arg(format!("{:.3}", start_ms as f64 / 1000.0))
        .args(["-t", &format!("{:.3}", (end_ms - start_ms) as f64 / 1000.0)])
        .arg("-i")
        .arg(audio)
        .args([
            "-ac",
            "1",
            "-ar",
            "16000",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-",
        ])
        .output()
        .context("ffmpeg failed to start")?;
    if !out.status.success() {
        bail!("ffmpeg could not cut {start_ms}..{end_ms}");
    }
    if out.stdout.is_empty() {
        bail!("empty chunk at {start_ms}");
    }
    Ok(out.stdout)
}

/// The same audio as opus, for the wire only.
///
/// Ten minutes is 19MB as PCM and 1.7MB at 24kbps, and the endpoint accepts
/// either — so this is a 25MB base64 body turned into 2MB, times one request
/// per ten minutes of every film in the library.
fn encode_opus(pcm: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut child = Command::new("ffmpeg")
        .args([
            "-v", "error", "-f", "s16le", "-ar", "16000", "-ac", "1", "-i", "-", "-c:a", "libopus",
            "-b:a", "24k", "-f", "ogg", "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("ffmpeg failed to start")?;
    let mut stdin = child.stdin.take().context("no ffmpeg stdin")?;
    let pcm = pcm.to_vec();
    // The pipe blocks once its buffer fills, so the writer cannot be the same
    // thread that drains stdout.
    let writer = std::thread::spawn(move || stdin.write_all(&pcm));
    let out = child.wait_with_output()?;
    let _ = writer.join();
    if !out.status.success() || out.stdout.is_empty() {
        bail!("ffmpeg could not encode the chunk to opus");
    }
    Ok(out.stdout)
}

/// Identify a film's audio by its *encoded* content, ignoring the container.
///
/// This is what the cache key rests on, so it has to be exactly reproducible
/// forever. Two things could spoil that, and this skips both. ffmpeg stamps a
/// random serial number into every Ogg page it writes, so the same audio
/// extracted twice is a different file byte for byte — so the page headers are
/// skipped and only the packets underneath are hashed. And decoding is not
/// bit-exact by specification: RFC 6716 defines conformance against test
/// vectors with a *tolerance*, and fixed-point and float builds legitimately
/// disagree — so nothing here decodes at all. A newer ffmpeg, a different CPU,
/// or a pure-Rust decoder cannot move this number, because none of them are
/// consulted.
///
/// A file that is not Ogg at all is hashed whole. That is still stable, which
/// is all the key needs; it just is not serial-independent, and nothing else
/// in the corpus produces one.
pub fn source_digest(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut hash = xxhash_rust::xxh3::Xxh3::new();
    let mut header = [0u8; 27];

    loop {
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        if &header[..4] != b"OggS" {
            // Not (or no longer) an Ogg stream. Hash the whole file instead of
            // guessing where the next page starts: a digest of the wrong bytes
            // is worse than a digest of all of them.
            let mut whole = std::io::BufReader::new(std::fs::File::open(path)?);
            let mut hash = xxhash_rust::xxh3::Xxh3::new();
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = whole.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
            return Ok(format!("{:016x}", hash.digest()));
        }
        // Byte 26 is the segment count; the table that follows gives each
        // packet fragment's length, and their sum is the payload.
        let segments = header[26] as usize;
        let mut table = vec![0u8; segments];
        file.read_exact(&mut table)?;
        let payload_len: usize = table.iter().map(|&n| n as usize).sum();
        let mut payload = vec![0u8; payload_len];
        file.read_exact(&mut payload)?;
        // The table is hashed too: it is how the payload divides into packets,
        // which is part of the content and not of the container's bookkeeping.
        hash.update(&table);
        hash.update(&payload);
    }
    Ok(format!("{:016x}", hash.digest()))
}

/// Every input that can change the response, in one canonical value.
///
/// The cache key is a hash of this, so adding or changing a parameter
/// invalidates exactly the entries it affects, automatically. The alternative
/// — a hand-maintained version string — is one forgotten bump away from
/// serving transcripts produced under settings nobody chose.
#[derive(Serialize)]
struct RequestSpec<'a> {
    endpoint: &'a str,
    model_id: &'a str,
    language_code: &'a str,
    timestamps_granularity: &'a str,
    diarize: bool,
    no_verbatim: bool,
    /// Which audio was asked about: the source film by content, and the
    /// stretch of it this request covers.
    ///
    /// Deliberately not a hash of what goes on the wire, and no longer a hash
    /// of the decoded samples either. Both are the output of a decode-and-
    /// re-encode that nobody guarantees is reproducible; see [`source_digest`].
    /// Naming the *region of a known file* instead is stable by construction,
    /// and it is the honest identity anyway — an ffmpeg upgrade that shifts a
    /// sample by one bit has not changed which audio we are asking about.
    audio_source_xxh3: &'a str,
    audio_start_ms: i64,
    audio_end_ms: i64,
}

/// The key this chunk used before the audio was identified by its source.
///
/// Kept so the entries already paid for are still found. It hashes the decoded
/// samples, so it can only be computed by decoding — but the caller has the
/// samples in hand already, and a lookup under it happens only when the new
/// key missed, which is exactly when the alternative is spending money.
#[derive(Serialize)]
struct LegacySpec<'a> {
    endpoint: &'a str,
    model_id: &'a str,
    language_code: &'a str,
    timestamps_granularity: &'a str,
    diarize: bool,
    no_verbatim: bool,
    audio_xxh3: String,
}

impl LegacySpec<'_> {
    fn key(&self) -> Result<String> {
        let canonical = serde_json::to_vec(self)?;
        Ok(format!(
            "asr/{}/{}/{:016x}",
            self.model_id,
            self.language_code,
            xxh3_64(&canonical)
        ))
    }
}

impl RequestSpec<'_> {
    fn key(&self) -> Result<String> {
        let canonical = serde_json::to_vec(self)?;
        Ok(format!(
            "asr/{}/{}/{:016x}",
            self.model_id,
            self.language_code,
            xxh3_64(&canonical)
        ))
    }
}

/// One stretch of one film's audio: the samples to send, and the two ways of
/// saying which audio they are.
struct Chunk<'a> {
    pcm: &'a [u8],
    source_xxh3: &'a str,
    start_ms: i64,
    end_ms: i64,
}

/// Transcribe one chunk, reading the shared cache first.
///
/// The cache holds the provider's **raw response body**, unparsed. Caching a
/// filtered or reshaped view would mean paying for the audio again every time
/// we change our mind about what to keep — and we already changed it once,
/// when audio events and per-word confidence turned out to be worth having.
async fn transcribe_chunk(
    http: &reqwest::Client,
    account: &ScribeAccount,
    store: &osmo::Store,
    chunk: Chunk<'_>,
    language: &str,
) -> Result<Vec<ScribeWord>> {
    let spec = RequestSpec {
        endpoint: SCRIBE_URL,
        model_id: SCRIBE_MODEL,
        language_code: language,
        timestamps_granularity: GRANULARITY,
        diarize: DIARIZE,
        no_verbatim: NO_VERBATIM,
        audio_source_xxh3: chunk.source_xxh3,
        audio_start_ms: chunk.start_ms,
        audio_end_ms: chunk.end_ms,
    };
    let key = spec.key()?;

    let legacy = LegacySpec {
        endpoint: SCRIBE_URL,
        model_id: SCRIBE_MODEL,
        language_code: language,
        timestamps_granularity: GRANULARITY,
        diarize: DIARIZE,
        no_verbatim: NO_VERBATIM,
        audio_xxh3: format!("{:016x}", xxh3_64(chunk.pcm)),
    };

    let cached = match store.read(&key).await {
        Some(bytes) => Some(bytes),
        // Only now, and only because the alternative is being billed: look
        // under the old key and carry anything found forward, so the change of
        // key costs nothing rather than re-buying what is already paid for.
        None => match store.read(&legacy.key()?).await {
            Some(bytes) => {
                store.write(&key, &bytes).await.ok();
                Some(bytes)
            }
            None => None,
        },
    };

    let raw = match cached {
        Some(bytes) => bytes,
        None => {
            let form = reqwest::multipart::Form::new()
                .text("model_id", spec.model_id.to_string())
                .text("language_code", spec.language_code.to_string())
                .text(
                    "timestamps_granularity",
                    spec.timestamps_granularity.to_string(),
                )
                .text("diarize", spec.diarize.to_string())
                // Opus on the wire; see `encode_opus`. Scribe accepts ogg.
                .part(
                    "file",
                    reqwest::multipart::Part::bytes(encode_opus(chunk.pcm)?)
                        .file_name("chunk.ogg")
                        .mime_str("audio/ogg")?,
                );
            let bytes = http
                .post(SCRIBE_URL)
                .header("xi-api-key", account.key())
                .multipart(form)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?
                .to_vec();
            // Parse before caching: a body that cannot be read is not worth
            // storing, and storing it would make the failure permanent.
            let _: ScribeResponse = serde_json::from_slice(&bytes)
                .context("could not parse the transcription response")?;
            store
                .write(&key, &bytes)
                .await
                .with_context(|| format!("could not cache {key}"))?;
            bytes
        }
    };

    let parsed: ScribeResponse = serde_json::from_slice(&raw)?;
    // `spacing` entries carry no content and would inflate every word count.
    Ok(parsed
        .words
        .into_iter()
        .filter(|w| w.r#type != "spacing" && !w.text.trim().is_empty())
        .collect())
}

/// Transcribe a whole film into words timed from its start.
pub async fn transcribe_film(
    http: &reqwest::Client,
    account: &ScribeAccount,
    store: &osmo::Store,
    audio: &Path,
    profile: &[f32],
    film_ms: i64,
    provenance: Provenance,
) -> Result<Transcript> {
    let language = provenance.language.as_str();
    let mut words = Vec::new();
    let mut failed = 0usize;
    let bounds = chunk_bounds(profile, film_ms);
    // Once per film, not once per chunk: it reads the whole file.
    let source_xxh3 = source_digest(audio)?;
    for (start_ms, end_ms) in &bounds {
        let pcm = match slice_pcm(audio, *start_ms, *end_ms) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("      chunk at {}s could not be cut: {e}", start_ms / 1000);
                failed += 1;
                continue;
            }
        };
        let chunk = Chunk {
            pcm: &pcm,
            source_xxh3: &source_xxh3,
            start_ms: *start_ms,
            end_ms: *end_ms,
        };
        match transcribe_chunk(http, account, store, chunk, language).await {
            Ok(heard) => words.extend(heard.into_iter().map(|w| Spoken {
                at_ms: start_ms + (w.start * 1000.0) as i64,
                until_ms: start_ms + (w.end.max(w.start) * 1000.0) as i64,
                kind: if w.r#type == "audio_event" {
                    Kind::AudioEvent
                } else {
                    Kind::Word
                },
                // Speaker ids are per-request, so they are namespaced by the
                // chunk they came from; otherwise `speaker_0` from minute 3
                // and `speaker_0` from minute 13 would look like one person.
                speaker: w.speaker_id.map(|s| format!("{}@{}", s, start_ms / 1000)),
                logprob: w.logprob,
                text: w.text,
                // Per-character spans are requested and therefore sit in the
                // cached raw response, but are not written into the artifact:
                // they double its size and nothing reads them yet. Because the
                // cache holds the raw body, adding them later costs a re-parse
                // rather than a re-transcription.
            })),
            Err(e) => {
                eprintln!("      chunk at {}s failed: {e}", start_ms / 1000);
                failed += 1;
            }
        }
    }
    // A transcript with holes is worse than none: the holes are invisible
    // downstream, and every cue inside one reads as dialogue nobody spoke.
    if failed > 0 {
        bail!("{failed} of {} chunks failed", bounds.len());
    }
    words.sort_by_key(|w| w.at_ms);
    Ok(Transcript { provenance, words })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cache key is money. Every entry under a given key was paid for
    /// once, and a key that moves for any reason other than a deliberate
    /// change of settings silently re-bills the whole corpus — about $250 at
    /// the rate we measured. Pinning it means that cost has to be typed into
    /// the diff by whoever incurs it, instead of arriving on a statement.
    #[test]
    fn cache_key_is_pinned() {
        let spec = RequestSpec {
            endpoint: SCRIBE_URL,
            model_id: SCRIBE_MODEL,
            language_code: "en",
            timestamps_granularity: GRANULARITY,
            diarize: DIARIZE,
            no_verbatim: NO_VERBATIM,
            audio_source_xxh3: "0000000000000000",
            audio_start_ms: 0,
            audio_end_ms: 600_000,
        };
        assert_eq!(spec.key().unwrap(), "asr/scribe_v2/en/375f151a696e4a16");
    }

    /// The legacy key must stay exactly where it was, or the fallback that
    /// rescues already-paid-for entries looks in the wrong place and they are
    /// bought a second time.
    #[test]
    fn legacy_key_still_resolves() {
        let spec = LegacySpec {
            endpoint: SCRIBE_URL,
            model_id: SCRIBE_MODEL,
            language_code: "en",
            timestamps_granularity: GRANULARITY,
            diarize: DIARIZE,
            no_verbatim: NO_VERBATIM,
            audio_xxh3: "0000000000000000".to_string(),
        };
        assert_eq!(spec.key().unwrap(), "asr/scribe_v2/en/25d1e40fd8719fb9");
    }

    /// One Ogg page carrying one packet, with a caller-chosen stream serial.
    fn ogg_page(serial: u32, seq: u32, payload: &[u8]) -> Vec<u8> {
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0); // version
        page.push(0); // header type
        page.extend_from_slice(&0u64.to_le_bytes()); // granule position
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&seq.to_le_bytes());
        page.extend_from_slice(&0u32.to_le_bytes()); // CRC
        page.push(1); // one segment
        page.push(payload.len() as u8);
        page.extend_from_slice(payload);
        page
    }

    /// The whole reason the key stopped hashing decoded samples: ffmpeg picks
    /// a random stream serial every time it writes an Ogg file, so the same
    /// audio extracted twice differs byte for byte. Identical packets must
    /// digest identically regardless.
    #[test]
    fn digest_ignores_the_stream_serial() {
        let dir = std::env::temp_dir().join("yap-source-digest-test");
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.opus"), dir.join("b.opus"));
        std::fs::write(
            &a,
            [
                ogg_page(0xDEAD_BEEF, 0, b"packet-one"),
                ogg_page(0xDEAD_BEEF, 1, b"two"),
            ]
            .concat(),
        )
        .unwrap();
        std::fs::write(
            &b,
            [
                ogg_page(0x0BAD_F00D, 0, b"packet-one"),
                ogg_page(0x0BAD_F00D, 1, b"two"),
            ]
            .concat(),
        )
        .unwrap();
        assert_eq!(source_digest(&a).unwrap(), source_digest(&b).unwrap());

        // ...but different audio must still differ, or the digest is useless.
        let c = dir.join("c.opus");
        std::fs::write(
            &c,
            [
                ogg_page(0xDEAD_BEEF, 0, b"packet-ONE"),
                ogg_page(0xDEAD_BEEF, 1, b"two"),
            ]
            .concat(),
        )
        .unwrap();
        assert_ne!(source_digest(&a).unwrap(), source_digest(&c).unwrap());
    }

    /// Provenance has to move when the settings do, or a stale transcript
    /// survives a change that should have replaced it.
    #[test]
    fn provenance_tracks_settings() {
        assert_ne!(
            provenance("en", None).unwrap().settings,
            provenance("ja", None).unwrap().settings
        );
    }

    /// A flat profile has no quiet seam to find, so the cuts fall back to the
    /// target mark — and must still tile the film exactly, with no gap and no
    /// overlap, or words go missing between chunks.
    #[test]
    fn bounds_tile_the_film() {
        let film_ms = 7_200_000;
        let profile = vec![0.5f32; (film_ms / BUCKET_MS) as usize];
        let bounds = chunk_bounds(&profile, film_ms);
        assert_eq!(bounds[0].0, 0);
        assert_eq!(bounds.last().unwrap().1, film_ms);
        for pair in bounds.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "chunks must be contiguous");
        }
    }

    /// A film shorter than one chunk is a single request.
    #[test]
    fn short_film_is_one_chunk() {
        let film_ms = 400_000;
        let profile = vec![0.5f32; (film_ms / BUCKET_MS) as usize];
        assert_eq!(chunk_bounds(&profile, film_ms), vec![(0, film_ms)]);
    }

    /// The seam should be pulled to the quiet patch, not left at the mark.
    #[test]
    fn cuts_move_to_the_quietest_seam() {
        let film_ms = 1_500_000;
        let mut profile = vec![0.9f32; (film_ms / BUCKET_MS) as usize];
        // A silence 30s before the 10-minute mark.
        let quiet_at = ((TARGET_CHUNK_MS - 30_000) / BUCKET_MS) as usize;
        for slot in profile.iter_mut().skip(quiet_at).take(20) {
            *slot = 0.01;
        }
        let cut = chunk_bounds(&profile, film_ms)[0].1;
        let quiet_ms = quiet_at as i64 * BUCKET_MS;
        assert!(
            (cut - quiet_ms).abs() < 2_000,
            "cut at {cut} should sit in the silence at {quiet_ms}"
        );
    }
}
