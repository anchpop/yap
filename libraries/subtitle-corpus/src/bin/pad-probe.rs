//! Diagnostic: what do the two speech witnesses say about the audio just
//! before a sentence the clear-margin gate rejected?
//!
//! For a sample of "neighbouring speech too close" rejects (and passes with a
//! wide margin as the control), cut the clip with a forced 400 ms lead-in
//! and read that lead-in with earshot (16 ms frames) and with the phoneme
//! model's frame matrix (blank probability per frame). Prints per-group
//! quantiles and how often the two agree that a 100 ms pause exists.
use std::path::{Path, PathBuf};

use language_utils::Language;
use phoneme_verify::VerifyContext;
use subtitle_corpus::clips::{read_clips, Clip};
use subtitle_corpus::cues::slice_wav_padded;
use subtitle_corpus::library::{course_dir, read_plan};

const LEAD_MS: i64 = 400;
const PAUSE_MS: f64 = 100.0;

fn wav_samples(wav: &[u8]) -> Vec<i16> {
    let mut at = 12;
    while at + 8 <= wav.len() {
        let len = u32::from_le_bytes(wav[at + 4..at + 8].try_into().unwrap()) as usize;
        if &wav[at..at + 4] == b"data" {
            return wav[at + 8..(at + 8 + len).min(wav.len())]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();
        }
        at += 8 + len + len % 2;
    }
    Vec::new()
}

struct Reading {
    /// earshot: share of lead frames below the clip's own speech level.
    ear_quiet: f64,
    /// earshot: longest run of such frames, ms.
    ear_run_ms: f64,
    /// phoneme model: share of lead frames it hears as speech.
    ph_speech: f64,
    /// phoneme model: a window of PAUSE_MS with zero speech frames exists.
    ph_pause: bool,
    /// The PAUSE_MS touching the sentence start is speech-free.
    ph_adjacent: bool,
    ear_adjacent: bool,
}

async fn read(ctx: &VerifyContext<'_>, audio: &Path, c: &Clip) -> anyhow::Result<Reading> {
    let pad_after = c.pad_after_ms.max(0);
    let wav = slice_wav_padded(audio, c.start_ms, c.end_ms, LEAD_MS, pad_after)?;
    let samples = wav_samples(&wav);
    let padded_ms = (c.end_ms - c.start_ms + LEAD_MS + pad_after) as f64;

    // earshot over the whole cut; threshold halfway between the cut's
    // floor and the spoken span's median, so each clip carries its own mix.
    let mut det = earshot::Detector::default_boxed();
    let scores: Vec<f32> = samples
        .chunks_exact(256)
        .map(|f| det.predict_i16(f))
        .collect();
    let lead_n = (LEAD_MS as usize * 16) / 256; // 25 frames
    let span_from = lead_n;
    let span_to = scores.len().saturating_sub((pad_after as usize * 16) / 256);
    let mut span: Vec<f32> = scores[span_from.min(scores.len())..span_to.max(span_from)].to_vec();
    span.sort_by(f32::total_cmp);
    let speech_level = span.get(span.len() / 2).copied().unwrap_or(1.0);
    let floor = scores.iter().copied().fold(f32::INFINITY, f32::min);
    // FIXED=0.5 in the environment tries a plain absolute cut instead.
    let threshold = std::env::var("FIXED")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or((speech_level + floor) / 2.0);
    let lead = &scores[..lead_n.min(scores.len())];
    let quiet = lead.iter().filter(|&&s| s < threshold).count();
    let (mut run, mut best) = (0usize, 0usize);
    for &s in lead {
        if s < threshold {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }

    let frames = phoneme_verify::frame_matrix(ctx, &wav).await?;
    let frame_ms = padded_ms / frames.frames as f64;
    let lead_frames = (LEAD_MS as f64 / frame_ms) as usize;
    let ph_speech = frames.speech_fraction(0, lead_frames).unwrap_or(1.0);
    let w = (PAUSE_MS / frame_ms).ceil() as usize;
    let ph_pause =
        (0..lead_frames.saturating_sub(w)).any(|i| frames.speech_fraction(i, i + w) == Some(0.0));
    // The gate's actual question: is the 100 ms touching the sentence clear?
    let ph_adjacent =
        frames.speech_fraction(lead_frames.saturating_sub(w), lead_frames) == Some(0.0);
    let adj_n = (PAUSE_MS / 16.0).ceil() as usize;
    let ear_adjacent =
        lead.len() >= adj_n && lead[lead.len() - adj_n..].iter().all(|&s| s < threshold);

    Ok(Reading {
        ear_quiet: quiet as f64 / lead.len().max(1) as f64,
        ear_run_ms: best as f64 * 16.0,
        ph_speech,
        ph_pause,
        ph_adjacent,
        ear_adjacent,
    })
}

fn quantiles(xs: &mut [f64]) -> String {
    xs.sort_by(f64::total_cmp);
    let q = |f: f64| xs[((xs.len() - 1) as f64 * f) as usize];
    format!("p25={:.2} p50={:.2} p75={:.2}", q(0.25), q(0.5), q(0.75))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/data/andrep/subtitle-corpus");
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    let plan = read_plan(&out)?;
    let mut rejects = Vec::new();
    let mut passes = Vec::new();
    for m in &plan {
        let dir = out.join(&m.imdb_id);
        let Ok(clips) = read_clips(&dir.join("clips.jsonl")) else {
            continue;
        };
        let code = course_dir(&m.original_language).unwrap();
        for c in clips {
            if c.reject.as_deref() == Some("neighbouring speech too close to cut clean")
                && c.clear_before_ms > 0
                && c.clear_before_ms < 100
            {
                rejects.push((code, dir.clone(), c));
            } else if c.passed && c.clear_before_ms >= 300 {
                passes.push((code, dir.clone(), c));
            }
        }
    }
    // Deterministic spread: take every k-th.
    fn pick(v: Vec<(&'static str, PathBuf, Clip)>, n: usize) -> Vec<(&'static str, PathBuf, Clip)> {
        let k = (v.len() / n).max(1);
        v.into_iter().step_by(k).take(n).collect()
    }
    let (rejects, passes) = (pick(rejects, n), pick(passes, n));

    let store = osmo::Store::open("./.cache");
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let empty = std::collections::HashMap::new();

    for (label, group) in [
        ("too-close rejects", rejects),
        ("passes ≥300ms clear", passes),
    ] {
        let mut ear_quiet = Vec::new();
        let mut ear_run = Vec::new();
        let mut ph_speech = Vec::new();
        let (mut ear_pause, mut ph_pause, mut both, mut neither) = (0, 0, 0, 0);
        let (mut ear_adj, mut ph_adj, mut both_adj) = (0, 0, 0);
        let mut done = 0usize;
        for (code, dir, c) in &group {
            let language = Language::from_code(code).unwrap();
            let ctx = VerifyContext::new(&http, store.clone(), &empty, language)?;
            let r = match read(&ctx, &dir.join("audio.opus"), c).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("  skip {}: {e:#}", c.sentence);
                    continue;
                }
            };
            done += 1;
            let ep = r.ear_run_ms >= PAUSE_MS;
            ear_pause += usize::from(ep);
            ph_pause += usize::from(r.ph_pause);
            both += usize::from(ep && r.ph_pause);
            neither += usize::from(!ep && !r.ph_pause);
            ear_adj += usize::from(r.ear_adjacent);
            ph_adj += usize::from(r.ph_adjacent);
            both_adj += usize::from(r.ear_adjacent && r.ph_adjacent);
            ear_quiet.push(r.ear_quiet);
            ear_run.push(r.ear_run_ms);
            ph_speech.push(r.ph_speech);
        }
        println!("== {label}: n={done}");
        println!(
            "  earshot   quiet share of lead {}; longest quiet run ms {}; ≥{PAUSE_MS}ms pause: {:.0}%",
            quantiles(&mut ear_quiet),
            quantiles(&mut ear_run),
            100.0 * ear_pause as f64 / done as f64
        );
        println!(
            "  phoneme   speech share of lead {}; ≥{PAUSE_MS}ms pause: {:.0}%",
            quantiles(&mut ph_speech),
            100.0 * ph_pause as f64 / done as f64
        );
        println!(
            "  adjacent {PAUSE_MS}ms before the sentence is clear: earshot {:.0}%, phoneme {:.0}%, both {:.0}%",
            100.0 * ear_adj as f64 / done as f64,
            100.0 * ph_adj as f64 / done as f64,
            100.0 * both_adj as f64 / done as f64
        );
        println!(
            "  agree: both see a pause {:.0}%, neither {:.0}%, only earshot {:.0}%, only phoneme {:.0}%",
            100.0 * both as f64 / done as f64,
            100.0 * neither as f64 / done as f64,
            100.0 * (ear_pause - both) as f64 / done as f64,
            100.0 * (ph_pause - both) as f64 / done as f64
        );
    }
    Ok(())
}
