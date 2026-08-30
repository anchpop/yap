//! Sentence → clip mapping: for every sentence a film's subtitle contains,
//! the span of audio in which it is spoken, verified twice.
//!
//! A sentence earns a clip only when two independent witnesses agree with
//! the subtitle:
//!
//! 1. **The transcript.** The sentence's tokens are aligned to the full-film
//!    word-timed transcript ([`cues::align_sentence`]); the run of words that
//!    spells it must do so near-verbatim, sit clear of neighbouring speech and
//!    of any audio event, and its word stamps give the clip its bounds. Cue
//!    times are display times, authored for reading; word stamps come from
//!    something that heard the speech, and a cue holding two sentences has no
//!    per-sentence timing at all.
//! 2. **The phoneme model.** The clip is cut and the model's per-frame
//!    distribution fetched ([`phoneme_verify::frame_matrix`]); the espeak
//!    rendering of the sentence is scored under CTC against the model's own
//!    free reading. Given (1) the words are right, so what this gate rejects
//!    is audio a listener can't actually make out — music beds, mumbling,
//!    heavy compression — which a transcription model is too robust to
//!    notice.
//!
//! Sentences are the ones course ingestion would produce from the same
//! subtitle (`movie_subtitles::segment`), keyed the same way, so a language
//! pack can look its sentences' clips up by text — independent of *which*
//! subtitle file a sentence came from.
//!
//! One `clips.jsonl` per film beside its transcript: a provenance line, then
//! one [`Clip`] per sentence that reached the phoneme gate, passing or not,
//! with every score kept so the gate can be re-tuned from the file alone.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use language_utils::Language;
use movie_subtitles::segment::{timed_passages, SubtitleSegmenter};
use movie_subtitles::{cleanup_subtitle_text, SubtitleLine};
use phoneme_verify::{FrameMatrix, VerifyContext};
use serde::{Deserialize, Serialize};

use crate::cues::{
    agreement_tokens, align_sentence, load_transcript, parse_cues, repair_latin_homoglyphs,
    slice_wav_padded, tokenization_for, AUDIO_PAD_MS, MATCH_SLOP_MS, MAX_CUE_MS, MIN_CUE_MS,
    MIN_TOKENS, POS_WER,
};
use crate::library::{course_dir, read_plan, Movie};
use crate::transcript::{Kind, Spoken};

/// Bump when the record format or the gating logic changes in a way that
/// makes existing `clips.jsonl` files not comparable.
const FORMAT_VERSION: u32 = 3;

/// What a film's `clips.jsonl` was computed from. A film is redone when any
/// of it changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub format: u32,
    pub subtitle_digest: String,
    pub transcript_digest: String,
    /// The phoneme model's cache partition (model revision + decoder).
    pub model: String,
    pub language: String,
    /// The gate the verdicts were made under. Frame matrices are cached, so
    /// re-gating under a new cut is cheap — and must happen, or a loosened
    /// cut would leave old verdicts standing.
    pub min_ratio: f64,
    pub min_clear_ms: i64,
}

/// One transcript word inside a clip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipWord {
    pub text: String,
    pub at_ms: i64,
    pub until_ms: i64,
}

/// A sentence and the audio span it was spoken in, with both witnesses'
/// verdicts. `passed` is the mapping's answer; everything else is why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub sentence: String,
    pub imdb_id: String,
    /// Clip bounds from the transcript's word stamps (unpadded).
    pub start_ms: i64,
    pub end_ms: i64,
    /// Padding the scored cut used on each side: up to [`AUDIO_PAD_MS`], but
    /// never more than half the silence to the neighbouring speech, so the
    /// cut carries no one else's onset or tail. Cut the same way to hear
    /// exactly what was scored.
    pub pad_before_ms: i64,
    pub pad_after_ms: i64,
    pub words: Vec<ClipWord>,
    /// Diarized speaker when every word in the span agrees on one.
    pub speaker: Option<String>,
    /// Token edit distance between sentence and span, over sentence tokens.
    pub transcript_wer: f64,
    pub audio_event_overlap: bool,
    /// Silence between the span and the nearest other speech, either side.
    pub clear_before_ms: i64,
    pub clear_after_ms: i64,
    /// espeak's rendering of the sentence — the CTC target.
    pub target_ipa: Vec<String>,
    /// Target tokens the model has no row for (the score is of the target
    /// without them).
    pub oov: Vec<String>,
    /// Log-odds per phoneme of the target against the model's free reading
    /// (0 = the target is what the model would have said).
    pub ratio: Option<f64>,
    pub logp_target_per_phoneme: Option<f64>,
    /// The model's free reading, for display.
    pub heard_ipa: Vec<String>,
    pub passed: bool,
    pub reject: Option<String>,
}

/// Per-film summary printed as films complete.
#[derive(Debug, Default, Clone, Copy)]
pub struct FilmSummary {
    pub sentences: usize,
    pub aligned: usize,
    pub scored: usize,
    pub passed: usize,
}

/// Gate settings.
#[derive(Debug, Clone)]
pub struct Gate {
    /// Lowest CTC log-odds ratio a clip may have and still pass; `None`
    /// takes the per-language default from [`default_min_ratio`].
    pub min_ratio: Option<f64>,
    /// Silence required on each side of the span. The pad shrinks to fit
    /// whatever silence there is; below this there is no room for a clean
    /// cut at all.
    pub min_clear_ms: i64,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            min_ratio: None,
            min_clear_ms: 60,
        }
    }
}

/// The CTC cut per language, from `phoneme-corpus-eval` on transcript-
/// verified vs transcript-rejected cues (model edcbbbf43a7f, 2026-08-29),
/// choosing the loosest cut that still keeps ≤ ~5–10% of rejected cues —
/// here the transcript has already thrown out the wrong-words cases, so
/// what the cut trades is yield against audio the model itself finds hard
/// to make out.
///
/// | lang | AUC  | cut  | verified kept | rejected kept |
/// |------|------|------|---------------|---------------|
/// | spa  | 0.95 | −1.0 | 65%           | 3%            |
/// | fra  | 0.93 | −1.0 | 62%           | 3%            |
/// | ita  | 0.92 | −1.0 | 56%           | 5%            |
/// | eng  | 0.82 | −1.0 | 50%           | 14%           |
/// | rus  | 0.93 | −1.0 | 58%           | 5%            |
/// | deu  | 0.89 | −1.5 | 72%           | 9%            |
/// | por  | 0.84 | −1.5 | 52%           | 8%            |
///
/// Russian first measured AUC 0.70 with verified cues at a median of −4.5:
/// the espeak parser was emitting palatalization `ʲ` as its own token
/// where the model's labels bind it to the consonant (`tʲ`), so every soft
/// consonant was a mismatch. With the parser fixed it behaves like French.
/// Languages absent here (hin, jpn, zho, kor, tha) have no espeak reference
/// the model was trained on; see lexide's PHONEME_BACKENDS.md.
pub fn default_min_ratio(code: &str) -> Option<f64> {
    Some(match code {
        "spa" | "fra" | "ita" | "eng" | "rus" => -1.0,
        "deu" | "por" => -1.5,
        _ => return None,
    })
}

pub fn clips_path(dir: &Path) -> PathBuf {
    dir.join("clips.jsonl")
}

fn stored_provenance(path: &Path) -> Option<Provenance> {
    let file = std::fs::File::open(path).ok()?;
    let mut first = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(file), &mut first).ok()?;
    serde_json::from_str(&first).ok()
}

/// Every clip in a film's `clips.jsonl` (the provenance line is skipped).
pub fn read_clips(path: &Path) -> Result<Vec<Clip>> {
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        .filter_map(|l| serde_json::from_str::<Clip>(l).ok())
        .collect())
}

/// The sentences of a subtitle track, with the time span of the passage each
/// came from — segmented exactly as course ingestion segments them.
fn subtitle_sentences(srt: &str, segmenter: &SubtitleSegmenter) -> Vec<(String, i64, i64)> {
    let lines: Vec<SubtitleLine> = parse_cues(srt)
        .into_iter()
        .filter_map(|cue| {
            let text = repair_latin_homoglyphs(&cleanup_subtitle_text(&cue.text));
            (!text.is_empty()).then_some(SubtitleLine {
                sentence: text,
                start_ms: cue.start_ms.max(0) as u32,
                end_ms: cue.end_ms.max(0) as u32,
            })
        })
        .collect();
    let mut out = Vec::new();
    for passage in timed_passages(&lines) {
        for sentence in segmenter.segment(&passage.text) {
            let sentence = sentence.trim().to_string();
            if sentence.is_empty() {
                continue;
            }
            out.push((
                sentence,
                i64::from(passage.start_ms),
                i64::from(passage.end_ms),
            ));
        }
    }
    out
}

/// A sentence's place in the transcript, or why it has none.
struct Placed {
    words: Vec<ClipWord>,
    speaker: Option<String>,
    wer: f64,
    audio_event_overlap: bool,
    clear_before_ms: i64,
    clear_after_ms: i64,
}

fn place(
    sentence: &str,
    passage_start: i64,
    passage_end: i64,
    transcript: &[Spoken],
    code: &str,
) -> std::result::Result<Placed, &'static str> {
    let tokenization = tokenization_for(code);
    let tokens = agreement_tokens(sentence, tokenization);
    if tokens.len() < MIN_TOKENS {
        return Err("too short");
    }
    // Digits poison both witnesses: the transcript spells numbers out and
    // espeak expands them its own way.
    if sentence.chars().any(|c| c.is_ascii_digit()) {
        return Err("contains a digit");
    }
    let (lo, hi) = (passage_start - MATCH_SLOP_MS, passage_end + MATCH_SLOP_MS);
    let window: Vec<(usize, &Spoken)> = transcript
        .iter()
        .enumerate()
        .filter(|(_, w)| w.kind == Kind::Word && w.at_ms < hi && w.until_ms > lo)
        .collect();
    let heard: Vec<String> = window
        .iter()
        .flat_map(|(_, w)| agreement_tokens(&w.text, tokenization))
        .collect();
    // Tokens and words are not 1:1 under `Chars` tokenization (or when a
    // transcript word holds punctuation-split pieces), so align on tokens and
    // map back to words through each word's token count.
    let word_of_token: Vec<usize> = window
        .iter()
        .enumerate()
        .flat_map(|(wi, (_, w))| {
            std::iter::repeat_n(wi, agreement_tokens(&w.text, tokenization).len())
        })
        .collect();
    let m = align_sentence(&tokens, &heard).ok_or("nothing heard")?;
    let wer = m.distance as f64 / tokens.len() as f64;
    if wer > POS_WER {
        return Err("transcript disagrees");
    }
    let (first_word, last_word) = (word_of_token[m.first], word_of_token[m.last]);
    let (first_idx, last_idx) = (window[first_word].0, window[last_word].0);
    let span = &transcript[first_idx..=last_idx];
    let start_ms = span[0].at_ms;
    let end_ms = span[span.len() - 1].until_ms;
    if !(MIN_CUE_MS..=MAX_CUE_MS).contains(&(end_ms - start_ms)) {
        return Err("span length out of bounds");
    }
    let audio_event_overlap = transcript
        .iter()
        .any(|w| w.kind == Kind::AudioEvent && w.at_ms < end_ms && w.until_ms > start_ms);
    let before = transcript[..first_idx]
        .iter()
        .rev()
        .find(|w| w.kind == Kind::Word)
        .map_or(i64::MAX, |w| start_ms - w.until_ms);
    let after = transcript[last_idx + 1..]
        .iter()
        .find(|w| w.kind == Kind::Word)
        .map_or(i64::MAX, |w| w.at_ms - end_ms);
    let mut speakers: Vec<&str> = span.iter().filter_map(|w| w.speaker.as_deref()).collect();
    speakers.dedup();
    Ok(Placed {
        words: span
            .iter()
            .filter(|w| w.kind == Kind::Word)
            .map(|w| ClipWord {
                text: w.text.clone(),
                at_ms: w.at_ms,
                until_ms: w.until_ms,
            })
            .collect(),
        speaker: match speakers.as_slice() {
            [only] => Some((*only).to_string()),
            _ => None,
        },
        wer,
        audio_event_overlap,
        clear_before_ms: before,
        clear_after_ms: after,
    })
}

/// Map one film. Returns the summary, or the reason nothing was done.
async fn clips_one(
    http: &reqwest::Client,
    store: &osmo::Store,
    movie: &Movie,
    dir: &Path,
    gate: &Gate,
    concurrency: usize,
) -> Result<FilmSummary> {
    let code = course_dir(&movie.original_language).context("unmapped language")?;
    let language = Language::from_code(code).context("unmapped course code")?;
    let subtitle = dir.join("subtitle.srt");
    let transcript_path = dir.join("transcript.jsonl");
    let audio = dir.join("audio.opus");
    for (what, p) in [
        ("subtitle", &subtitle),
        ("transcript", &transcript_path),
        ("audio", &audio),
    ] {
        if !p.exists() {
            bail!("no {what}");
        }
    }

    let Some(min_ratio) = gate.min_ratio.or_else(|| default_min_ratio(code)) else {
        bail!("no calibrated phoneme gate for {code}");
    };
    let provenance = Provenance {
        format: FORMAT_VERSION,
        subtitle_digest: crate::transcript::source_digest(&subtitle)?,
        transcript_digest: crate::transcript::source_digest(&transcript_path)?,
        model: phoneme_verify::production_cache_version(),
        language: code.to_string(),
        min_ratio,
        min_clear_ms: gate.min_clear_ms,
    };
    let path = clips_path(dir);
    if stored_provenance(&path).as_ref() == Some(&provenance) {
        let clips = read_clips(&path)?;
        return Ok(FilmSummary {
            sentences: clips.len(),
            aligned: clips.len(),
            scored: clips.len(),
            passed: clips.iter().filter(|c| c.passed).count(),
        });
    }

    let empty = std::collections::HashMap::new();
    let ctx = VerifyContext::new(http, store.clone(), &empty, language)?;
    let segmenter = SubtitleSegmenter::for_language(language)?;
    let transcript = load_transcript(&transcript_path)?;
    let sentences = subtitle_sentences(&std::fs::read_to_string(&subtitle)?, &segmenter);

    let mut summary = FilmSummary {
        sentences: sentences.len(),
        ..Default::default()
    };
    let placed: Vec<(String, Placed)> = sentences
        .iter()
        .filter_map(|(s, a, b)| {
            place(s, *a, *b, &transcript, code)
                .ok()
                .map(|p| (s.clone(), p))
        })
        .collect();
    summary.aligned = placed.len();

    use futures::StreamExt;
    let clips: Vec<Option<Clip>> = futures::stream::iter(placed)
        .map(|(sentence, p)| {
            let ctx = &ctx;
            let audio = audio.clone();
            let imdb_id = movie.imdb_id.clone();
            async move {
                let (start_ms, end_ms) = (p.words[0].at_ms, p.words[p.words.len() - 1].until_ms);
                let pad = |clear: i64| AUDIO_PAD_MS.min(clear / 2).max(0);
                let (pad_before_ms, pad_after_ms) = (pad(p.clear_before_ms), pad(p.clear_after_ms));
                let mut clip = Clip {
                    sentence: sentence.clone(),
                    imdb_id,
                    start_ms,
                    end_ms,
                    pad_before_ms,
                    pad_after_ms,
                    words: p.words,
                    speaker: p.speaker,
                    transcript_wer: p.wer,
                    audio_event_overlap: p.audio_event_overlap,
                    clear_before_ms: p.clear_before_ms,
                    clear_after_ms: p.clear_after_ms,
                    target_ipa: Vec::new(),
                    oov: Vec::new(),
                    ratio: None,
                    logp_target_per_phoneme: None,
                    heard_ipa: Vec::new(),
                    passed: false,
                    reject: None,
                };
                if clip.audio_event_overlap {
                    clip.reject = Some("audio event inside the span".into());
                    return Some(clip);
                }
                if clip.clear_before_ms < gate.min_clear_ms
                    || clip.clear_after_ms < gate.min_clear_ms
                {
                    clip.reject = Some("neighbouring speech too close to cut clean".into());
                    return Some(clip);
                }
                // espeak marks a language switch with `(en)`…`(fr)`; the
                // parser leaves the parentheses as tokens, which are not
                // phonemes.
                let target = match espeak::phonemize_phrase(&sentence, language) {
                    Ok(Some(t)) if !t.is_empty() => t
                        .into_iter()
                        .filter(|tok| tok != "(" && tok != ")")
                        .collect::<Vec<_>>(),
                    Ok(_) => {
                        clip.reject = Some("espeak produced no phonemes".into());
                        return Some(clip);
                    }
                    Err(e) => {
                        clip.reject = Some(format!("espeak: {e:#}"));
                        return Some(clip);
                    }
                };
                clip.target_ipa = target.clone();
                let wav = match tokio::task::spawn_blocking(move || {
                    slice_wav_padded(&audio, start_ms, end_ms, pad_before_ms, pad_after_ms)
                })
                .await
                .expect("slice task panicked")
                {
                    Ok(w) => w,
                    Err(e) => {
                        clip.reject = Some(format!("cut: {e:#}"));
                        return Some(clip);
                    }
                };
                let frames: FrameMatrix = match phoneme_verify::frame_matrix(ctx, &wav).await {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("  {}: {e:#}", clip.sentence);
                        return None;
                    }
                };
                let score = frames.score_target(&target);
                clip.heard_ipa = frames
                    .greedy_ids()
                    .into_iter()
                    .map(|id| frames.vocab[id].clone())
                    .collect();
                clip.oov = score.oov;
                clip.ratio = score.ratio;
                clip.logp_target_per_phoneme = score.logp_target_per_phoneme;
                clip.reject = match clip.ratio {
                    _ if !clip.oov.is_empty() => Some(format!(
                        "target phonemes outside the model vocabulary: {}",
                        clip.oov.join(" ")
                    )),
                    None => Some("target could not be scored".into()),
                    Some(r) if r < min_ratio => Some(format!("ratio {r:.2} below {min_ratio:.2}")),
                    Some(_) => None,
                };
                clip.passed = clip.reject.is_none();
                Some(clip)
            }
        })
        .buffered(concurrency.max(1))
        .collect()
        .await;

    let clips: Vec<Clip> = clips.into_iter().flatten().collect();
    summary.scored = clips.len();
    summary.passed = clips.iter().filter(|c| c.passed).count();

    let mut text = serde_json::to_string(&provenance)?;
    text.push('\n');
    for clip in &clips {
        text.push_str(&serde_json::to_string(clip)?);
        text.push('\n');
    }
    let tmp = dir.join("clips.jsonl.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(summary)
}

/// Map every transcribed film (or the ones selected), skipping films whose
/// `clips.jsonl` is already current.
pub async fn clips_all(
    out: PathBuf,
    films_in_flight: usize,
    limit: usize,
    imdb: Option<String>,
    langs: Option<Vec<String>>,
    gate: Gate,
) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut queue: Vec<Movie> = plan
        .into_iter()
        .filter(|m| imdb.as_deref().is_none_or(|id| m.imdb_id == id))
        .filter(|m| {
            langs.as_ref().is_none_or(|l| {
                course_dir(&m.original_language).is_some_and(|c| l.iter().any(|x| x == c))
            })
        })
        .filter(|m| {
            let dir = out.join(&m.imdb_id);
            dir.join("subtitle.srt").exists()
                && dir.join("transcript.jsonl").exists()
                && dir.join("audio.opus").exists()
        })
        .collect();
    if limit > 0 {
        queue.truncate(limit);
    }
    let total = queue.len();
    println!("{total} transcribed films to map");
    if total == 0 {
        return Ok(());
    }

    let store = Arc::new(osmo::Store::open("./.cache"));
    let http = Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?,
    );
    let out = Arc::new(out);
    let gate = Arc::new(gate);
    let progress = AtomicUsize::new(0);

    use futures::StreamExt;
    let totals: Vec<Option<FilmSummary>> = futures::stream::iter(queue)
        .map(|movie| {
            let (http, store, out, gate) = (
                Arc::clone(&http),
                Arc::clone(&store),
                Arc::clone(&out),
                Arc::clone(&gate),
            );
            let progress = &progress;
            async move {
                let dir = out.join(&movie.imdb_id);
                let outcome = clips_one(&http, &store, &movie, &dir, &gate, 8).await;
                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                let title = crate::library::truncate(&movie.title, 34);
                match &outcome {
                    Ok(s) => println!(
                        "[{n}/{total}] {title} ✓ {} sentences → {} placed → {} scored → {} pass",
                        s.sentences, s.aligned, s.scored, s.passed
                    ),
                    Err(e) => println!("[{n}/{total}] {title} ✗ {e:#}"),
                }
                outcome.ok()
            }
        })
        .buffer_unordered(films_in_flight.max(1))
        .collect()
        .await;

    let done: Vec<FilmSummary> = totals.into_iter().flatten().collect();
    println!(
        "\n{} films mapped: {} sentences, {} placed by the transcript, {} passed both gates",
        done.len(),
        done.iter().map(|s| s.sentences).sum::<usize>(),
        done.iter().map(|s| s.aligned).sum::<usize>(),
        done.iter().map(|s| s.passed).sum::<usize>()
    );
    Ok(())
}
