//! Aligning a downloaded subtitle to the file actually on disk.
//!
//! A subtitle fetched from OpenSubtitles carries the right words with the wrong
//! clock: it was timed against whatever release its author had. Measured
//! offsets on this library run from 0.4 to 14.2 seconds, which is fatal for
//! anything that cuts audio at a cue boundary.
//!
//! The fix is to find where a few of its lines are *actually* spoken. Whisper
//! returns per-word timestamps, so a phrase matched in the transcript yields an
//! exact time in this file; paired with the time the subtitle claims, that is
//! one anchor. Enough anchors determine both a constant shift and a rate — the
//! latter matters because a 23.976-to-25 fps mismatch stretches the whole track
//! and no single shift can repair it.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// One subtitle cue, text preserved exactly as written.
///
/// Deliberately not the cleaned [`movie_subtitles::SubtitleLine`]: the output
/// here is a subtitle file, so markup and line breaks have to survive. Cleaning
/// is only used to compare text.
#[derive(Debug, Clone)]
pub struct Cue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

fn parse_stamp(s: &str) -> Option<i64> {
    let s = s.trim().replace('.', ",");
    let (hms, ms) = s.split_once(',')?;
    let mut it = hms.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let sec: i64 = it.next()?.trim().parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms.trim().parse::<i64>().ok()?)
}

fn format_stamp(ms: i64) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn parse_cues(srt: &str) -> Vec<Cue> {
    // Course raw SRTs are LF, but a Bazarr sidecar keeps whatever the uploader
    // used — usually CRLF, where blank lines are `\r\n\r\n` and a `\n\n` block
    // split sees the whole file as one cue (Marnie: "covers only 0% of the
    // film" from a perfectly good 1,300-cue file).
    let srt = srt.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let srt = movie_subtitles::repair_cp1252_mojibake(&srt);
    let mut cues = Vec::new();
    for block in srt.split("\n\n") {
        let mut start = None;
        let mut end = None;
        let mut text = Vec::new();
        for line in block.lines() {
            if let Some((a, b)) = line.split_once("-->") {
                start = parse_stamp(a);
                end = parse_stamp(b);
            } else if !line.trim().is_empty() && line.trim().parse::<u32>().is_err() {
                text.push(line.trim());
            }
        }
        if let (Some(start), Some(end)) = (start, end) {
            if !text.is_empty() {
                cues.push(Cue {
                    start_ms: start,
                    end_ms: end,
                    text: text.join("\n"),
                });
            }
        }
    }
    cues
}

pub fn write_cues(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_stamp(c.start_ms),
            format_stamp(c.end_ms),
            c.text
        ));
    }
    out
}

/// Is this character from a script written without spaces between words?
fn spaceless_script(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF      // hiragana, katakana
        | 0x3400..=0x4DBF    // CJK extension A
        | 0x4E00..=0x9FFF    // CJK unified
        | 0xF900..=0xFAFF    // CJK compatibility
        | 0x0E00..=0x0E7F) // Thai
}

/// Comparable units of text: words where the script has them, character
/// n-grams where it does not.
///
/// Whisper punctuates and capitalises to its own taste and subtitle authors do
/// the same, so only the bare letters are worth comparing. Chinese, Japanese
/// and Thai write without spaces, so splitting on whitespace yields one huge
/// token per line that can never match — that is why those films produced zero
/// anchors. A short run of characters is the equivalent distinctive unit, and
/// 36% of the films awaiting alignment are in such scripts.
const NGRAM: usize = 4;

fn tokens(text: &str) -> Vec<String> {
    let lowered = text.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let spaceless = cleaned.chars().filter(|c| spaceless_script(*c)).count();
    let letters = cleaned.chars().filter(|c| c.is_alphanumeric()).count();
    if letters > 0 && spaceless * 2 > letters {
        let chars: Vec<char> = cleaned.chars().filter(|c| c.is_alphanumeric()).collect();
        return chars
            .windows(NGRAM)
            .map(|w| w.iter().collect::<String>())
            .collect();
    }
    cleaned.split_whitespace().map(str::to_owned).collect()
}

#[derive(Debug, Deserialize)]
struct WhisperWord {
    word: String,
    start: f64,
    #[serde(default)]
    end: f64,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    #[serde(default)]
    words: Vec<WhisperWord>,
}

#[derive(Debug, Deserialize)]
struct WhisperResult {
    #[serde(default)]
    segments: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperEnvelope {
    result: WhisperResult,
}

/// A transcribed word and the span it occupies, in milliseconds into the film.
///
/// The raw word is kept rather than a comparable token: in a script written
/// without spaces the useful unit spans several of Whisper's words, so the
/// stream has to be reassembled before it can be cut up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedWord {
    pub text: String,
    pub at_ms: i64,
    pub until_ms: i64,
}

/// How long the film runs, in milliseconds.
pub fn duration_ms(video: &Path) -> Result<i64> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(video)
        .output()
        .context("ffprobe failed to start")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let secs: f64 = text
        .trim()
        .parse()
        .with_context(|| format!("could not read duration from {text:?}"))?;
    Ok((secs * 1000.0) as i64)
}

/// The index of the audio stream in the language the film was made in.
pub fn original_audio_stream(video: &Path, codes: &[&str]) -> Result<usize> {
    #[derive(Deserialize)]
    struct Stream {
        #[serde(default)]
        tags: std::collections::HashMap<String, String>,
        #[serde(default)]
        disposition: std::collections::HashMap<String, u8>,
    }
    impl Stream {
        /// A director's commentary is in the film's language and comes
        /// first on plenty of discs (Delicatessen: track 1 is Jeunet
        /// talking over the film, track 3 is the film). Transcribing it
        /// yields a transcript that agrees with almost nothing.
        fn is_commentary(&self) -> bool {
            self.disposition.get("comment").copied().unwrap_or(0) == 1
                || self
                    .tags
                    .get("title")
                    .is_some_and(|t| t.to_lowercase().contains("commentary"))
        }
    }
    #[derive(Deserialize)]
    struct Probe {
        #[serde(default)]
        streams: Vec<Stream>,
    }
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream_tags=language,title:stream_disposition=comment",
            "-of",
            "json",
        ])
        .arg(video)
        .output()?;
    let probe: Probe = serde_json::from_slice(&out.stdout).unwrap_or(Probe { streams: vec![] });
    // Position among *audio* streams, which is what `-map 0:a:N` counts.
    for (i, s) in probe.streams.iter().enumerate() {
        if s.is_commentary() {
            continue;
        }
        let lang = s.tags.get("language").cloned().unwrap_or_default();
        let lang = lang.trim().to_lowercase();
        // "mul" is the original mixed track of a multilingual film.
        if codes.contains(&lang.as_str()) || lang == "mul" {
            return Ok(i);
        }
    }
    // Untagged audio on a single-track rip is the original often enough to try.
    if probe.streams.len() == 1 && !probe.streams[0].is_commentary() {
        return Ok(0);
    }
    bail!("no audio stream in the film's own language")
}

/// The exact audio stream an extraction came from, for its provenance stamp.
///
/// A remux can keep a video's name while reordering or replacing its audio, so
/// file identity alone is not enough: the stamp records which stream was read
/// and what it looked like, and any mismatch evicts the extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStreamIdentity {
    /// Position among the file's *audio* streams, as `-map 0:a:N` counts.
    pub stream_index: usize,
    pub codec: String,
    pub channels: u32,
    #[serde(default)]
    pub channel_layout: String,
}

pub fn audio_stream_identity(video: &Path, audio_stream: usize) -> Result<AudioStreamIdentity> {
    #[derive(Deserialize)]
    struct Stream {
        codec_name: Option<String>,
        #[serde(default)]
        channels: u32,
        #[serde(default)]
        channel_layout: String,
    }
    #[derive(Deserialize)]
    struct Probe {
        #[serde(default)]
        streams: Vec<Stream>,
    }
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            &format!("a:{audio_stream}"),
            "-show_entries",
            "stream=codec_name,channels,channel_layout",
            "-of",
            "json",
        ])
        .arg(video)
        .output()
        .context("ffprobe failed to start")?;
    let probe: Probe = serde_json::from_slice(&out.stdout).unwrap_or(Probe { streams: vec![] });
    let s = probe
        .streams
        .into_iter()
        .next()
        .with_context(|| format!("no audio stream a:{audio_stream}"))?;
    Ok(AudioStreamIdentity {
        stream_index: audio_stream,
        codec: s.codec_name.unwrap_or_default(),
        channels: s.channels,
        channel_layout: s.channel_layout,
    })
}

/// The Workers AI account to transcribe against.
///
/// Read once, at the top of a run, because the alternative is worse than it
/// looks: reading the environment per window turns a missing variable into a
/// per-window failure, every window of a film fails, and the film is recorded
/// `undecided` — a durable "we could not verify this" written by a typo.
/// Holding the credentials in a value the transcriber cannot be called
/// without makes that unrepresentable.
#[derive(Clone)]
pub struct WhisperAccount {
    account: String,
    token: String,
}

impl WhisperAccount {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            account: std::env::var("CLOUDFLARE_ACCOUNT_ID")
                .context("CLOUDFLARE_ACCOUNT_ID not set")?,
            token: std::env::var("CLOUDFLARE_WORKER_AI_API_TOKEN")
                .context("CLOUDFLARE_WORKER_AI_API_TOKEN not set")?,
        })
    }

    pub fn transcribe_url(&self) -> String {
        format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{WHISPER_MODEL}",
            self.account
        )
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

/// The Workers AI model every transcription in this crate goes through.
const WHISPER_MODEL: &str = "@cf/openai/whisper-large-v3-turbo";

/// Transcribe one window of the film, returning words timed from its start.
pub async fn transcribe_window(
    http: &reqwest::Client,
    account: &WhisperAccount,
    video: &Path,
    audio_stream: usize,
    start_ms: i64,
    length_s: u32,
    language: &str,
) -> Result<Vec<TimedWord>> {
    let tmp = std::env::temp_dir().join(format!("subsync-{}.wav", std::process::id()));
    // Seeking before `-i` decodes only from that point, so a window costs
    // seconds rather than a pass over the whole film.
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-ss"])
        .arg(format!("{:.3}", start_ms as f64 / 1000.0))
        .args(["-t", &length_s.to_string(), "-i"])
        .arg(video)
        .args([
            "-map",
            &format!("0:a:{audio_stream}"),
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&tmp)
        .status()?;
    if !status.success() {
        bail!("ffmpeg could not extract the audio window");
    }
    let wav = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);

    let url = account.transcribe_url();
    let token = account.token();
    use base64::Engine;
    let body = serde_json::json!({
        "audio": base64::engine::general_purpose::STANDARD.encode(&wav),
        "language": language,
    });
    let response = http
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let envelope: WhisperEnvelope = response.json().await?;

    Ok(envelope
        .result
        .segments
        .into_iter()
        .flat_map(|s| s.words)
        .map(|w| TimedWord {
            at_ms: start_ms + (w.start * 1000.0) as i64,
            until_ms: start_ms + (w.end.max(w.start) * 1000.0) as i64,
            text: w.word,
        })
        .collect())
}

/// A subtitle line located in the transcript: what it claims, and when it is
/// really said.
#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub subtitle_ms: i64,
    pub spoken_ms: i64,
}

/// Pair distinctive words between the subtitle and the transcript.
///
/// Matching whole lines verbatim does not work: Whisper mishears or
/// repunctuates one word and the entire line is lost. On a real film that
/// yielded *zero* matches where word-level pairing yielded plenty.
///
/// So the unit is a single word that is long enough to be distinctive and
/// occurs exactly once on each side — "énergumène" can only be one moment,
/// while "oui" could be any of two hundred. A misheard word simply fails to
/// pair instead of destroying its neighbours.
pub fn find_anchors(cues: &[Cue], heard: &[TimedWord], min_len: usize) -> Vec<Anchor> {
    let spoken = comparable_stream(heard);
    let mut heard_count: std::collections::HashMap<&str, usize> = Default::default();
    for (token, _) in &spoken {
        *heard_count.entry(token.as_str()).or_default() += 1;
    }

    // Where each subtitle word falls, interpolated across its cue. Taking the
    // cue's start for every word would add up to a cue's length of error to
    // each anchor, which is the same order as the offsets being measured.
    let mut when_written: std::collections::HashMap<String, Vec<i64>> = Default::default();
    for cue in cues {
        let words = tokens(&movie_subtitles::cleanup_subtitle_text(&cue.text));
        let span = (cue.end_ms - cue.start_ms).max(0) as f64;
        for (i, w) in words.iter().enumerate() {
            let at = cue.start_ms + (span * i as f64 / words.len().max(1) as f64) as i64;
            when_written.entry(w.clone()).or_default().push(at);
        }
    }

    spoken
        .iter()
        .filter(|(t, _)| t.chars().count() >= min_len)
        .filter(|(t, _)| heard_count.get(t.as_str()) == Some(&1))
        .filter_map(|(t, at)| {
            let times = when_written.get(t)?;
            (times.len() == 1).then_some(Anchor {
                subtitle_ms: times[0],
                spoken_ms: *at,
            })
        })
        .collect()
}

/// The transcript as comparable tokens, each with the time it is spoken.
///
/// For a space-less script the n-grams must run across the whole transcript,
/// not within each of Whisper's words: those are one to three characters long
/// in Japanese, so cutting 4-grams out of them individually yields nothing at
/// all. Characters are timed by interpolating across the word that contains
/// them.
fn comparable_stream(heard: &[TimedWord]) -> Vec<(String, i64)> {
    let joined: String = heard.iter().map(|w| w.text.as_str()).collect();
    let spaceless = joined.chars().filter(|c| spaceless_script(*c)).count();
    let letters = joined.chars().filter(|c| c.is_alphanumeric()).count();

    if letters > 0 && spaceless * 2 > letters {
        let mut chars: Vec<(char, i64)> = Vec::new();
        for w in heard {
            let cs: Vec<char> = w
                .text
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect();
            let span = (w.until_ms - w.at_ms).max(0);
            for (i, c) in cs.iter().enumerate() {
                chars.push((*c, w.at_ms + span * i as i64 / cs.len().max(1) as i64));
            }
        }
        return chars
            .windows(NGRAM)
            .map(|win| (win.iter().map(|(c, _)| *c).collect::<String>(), win[0].1))
            .collect();
    }

    heard
        .iter()
        .flat_map(|w| {
            let at = w.at_ms;
            tokens(&w.text).into_iter().map(move |t| (t, at))
        })
        .collect()
}

/// Where to listen: the densest stretch of dialogue within each part of the film.
///
/// Uniformly spaced windows land wherever they land, and a window over a quiet
/// or musical scene transcribes almost nothing — one such window returned 50
/// tokens in 60 seconds. The subtitle already says where the talking is, and
/// although its clock is wrong, it is wrong by seconds against a film that runs
/// for hours, so it points at the right stretches.
pub fn choose_windows(cues: &[Cue], duration_ms: i64, count: usize, window_s: u32) -> Vec<i64> {
    let window_ms = window_s as i64 * 1000;
    let count = count.max(1) as i64;
    let mut chosen = Vec::new();
    for part in 0..count {
        // Skip the outer eighths: logos, opening titles and end credits.
        let lo = duration_ms / 8 + (duration_ms * 3 / 4) * part / count;
        let hi = duration_ms / 8 + (duration_ms * 3 / 4) * (part + 1) / count;
        let mut best = (lo, -1i64);
        let mut at = lo;
        while at + window_ms <= hi.max(lo + window_ms) {
            let n = cues
                .iter()
                .filter(|c| c.start_ms >= at && c.start_ms < at + window_ms)
                .count() as i64;
            if n > best.1 {
                best = (at, n);
            }
            at += window_ms / 2;
        }
        chosen.push(best.0);
    }
    chosen
}

/// The mapping from subtitle time to real time: `spoken = rate * subtitle + offset`.
#[derive(Debug, Clone, Copy)]
pub struct Alignment {
    pub rate: f64,
    pub offset_ms: f64,
    pub anchors_used: usize,
    pub anchors_seen: usize,
    /// Largest residual among the anchors kept, in milliseconds.
    pub worst_residual_ms: f64,
}

impl Alignment {
    pub fn apply(&self, ms: i64) -> i64 {
        (self.rate * ms as f64 + self.offset_ms).round() as i64
    }
}

/// Fit an alignment, discarding anchors that disagree with the consensus.
///
/// A wrong anchor — a phrase matched in the wrong place — would drag a
/// least-squares fit badly, so the shift is first taken as the *mode* of the
/// per-anchor deltas and everything far from it dropped. Only then is a rate
/// fitted, and only when the surviving anchors span enough of the film for the
/// slope to mean anything.
pub fn fit(anchors: &[Anchor], tolerance_ms: f64) -> Option<Alignment> {
    if anchors.len() < 4 {
        return None;
    }
    let mut buckets: std::collections::HashMap<i64, usize> = Default::default();
    for a in anchors {
        *buckets
            .entry((a.spoken_ms - a.subtitle_ms) / 1000)
            .or_default() += 1;
    }
    let peak = *buckets.iter().max_by_key(|(_, n)| **n)?.0 as f64 * 1000.0;
    let inliers: Vec<&Anchor> = anchors
        .iter()
        .filter(|a| ((a.spoken_ms - a.subtitle_ms) as f64 - peak).abs() <= tolerance_ms)
        .collect();
    if inliers.len() < 4 {
        return None;
    }

    let n = inliers.len() as f64;
    let mean_x = inliers.iter().map(|a| a.subtitle_ms as f64).sum::<f64>() / n;
    let mean_y = inliers.iter().map(|a| a.spoken_ms as f64).sum::<f64>() / n;
    let sxx: f64 = inliers
        .iter()
        .map(|a| (a.subtitle_ms as f64 - mean_x).powi(2))
        .sum();
    let sxy: f64 = inliers
        .iter()
        .map(|a| (a.subtitle_ms as f64 - mean_x) * (a.spoken_ms as f64 - mean_y))
        .sum();
    // Anchors clustered in one part of the film cannot distinguish a rate from
    // a shift; assume rate 1 rather than fit noise. 10 minutes of spread.
    let spread = inliers
        .iter()
        .map(|a| a.subtitle_ms)
        .max()?
        .saturating_sub(inliers.iter().map(|a| a.subtitle_ms).min()?);
    let rate = if spread > 600_000 && sxx > 0.0 {
        (sxy / sxx).clamp(0.9, 1.1)
    } else {
        1.0
    };
    let offset = mean_y - rate * mean_x;

    let worst = inliers
        .iter()
        .map(|a| (a.spoken_ms as f64 - (rate * a.subtitle_ms as f64 + offset)).abs())
        .fold(0.0, f64::max);

    Some(Alignment {
        rate,
        offset_ms: offset,
        anchors_used: inliers.len(),
        anchors_seen: anchors.len(),
        worst_residual_ms: worst,
    })
}
