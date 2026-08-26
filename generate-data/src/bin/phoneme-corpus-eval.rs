//! Evaluate the wav2vec2 phoneme model on subtitle-corpus film audio.
//!
//! The model was trained on clean single-speaker recordings; film audio (music
//! beds, room tone, compression) is out of distribution, and before any clip
//! filter is built on it we need to know whether it still *discriminates*
//! there. The trick is that we don't need phonetic ground truth to measure
//! that: the full-film ElevenLabs transcript is an independent witness, so a
//! subtitle cue whose text the transcript confirms verbatim is a known-good
//! clip and a cue the transcript contradicts (or hears nothing for) is a
//! known-bad one. If the phoneme-vs-espeak edit distance separates those two
//! populations, the model works on this material regardless of whether either
//! side is "correct" IPA.
//!
//! For each film with all three artifacts (synced `subtitle.srt`, full
//! `transcript.jsonl`, `audio.opus`) in an espeak-supported language:
//!
//! 1. Label each cue by transcript agreement (token WER between the cleaned
//!    cue text and the transcript words overlapping its span): low WER →
//!    `pos`, high WER → `neg` (split into `neg_mismatch` / `neg_silent`),
//!    anything between → unlabeled and skipped.
//! 2. Sample up to a per-film quota of each label, spread across the film.
//! 3. Cut the cue's audio span from `audio.opus`, run it through the
//!    production `verify_clip_bytes` path (Modal wav2vec2, shared cache,
//!    same normalization) against the espeak phrase-level rendering as the
//!    sole expected sequence — no wikipron, so the comparison is purely
//!    heard-phonemes vs espeak, which is the production shape for corpus
//!    text where per-word dictionaries can't be assumed complete.
//! 4. Append one JSONL record per cue; print per-language separation stats
//!    and the substitution confusion tallies at the end.
//!
//! Predictions are cached under the production cache partition (keyed by the
//! WAV bytes), so re-runs and later analysis passes cost nothing.
//!
//! Usage (from the repo root, so `.cache` resolves):
//!     ESPEAK_NG_BIN=... ESPEAK_NG_DATA_PATH=... \
//!     cargo run --release --bin phoneme-corpus-eval -- [--langs fra,deu] [--max-films 2]

use anyhow::{Context, Result};
use clap::Parser;
use generate_data::audio_verification::{
    AlignmentOp, ClipVerification, VerifyContext, expected_phoneme_variants, verify_clip_bytes,
};
use generate_data::subtitle_corpus::{
    Candidate, CueLabel, MIN_FILM_POSITIVES, PlanEntry, Tokenization,
    course_code_espeak as course_code, label_cues, load_transcript, parse_cues, sample, slice_wav,
};
use language_utils::Language;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(about = "Evaluate the wav2vec2 phoneme model against transcript-labeled film cues")]
struct Args {
    /// Root of the subtitle corpus.
    #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
    corpus: PathBuf,
    /// Comma-separated course codes to include (default: every espeak-supported one).
    #[arg(long)]
    langs: Option<String>,
    /// Cues sampled per film per label.
    #[arg(long, default_value_t = 40)]
    per_film: usize,
    /// Stop after this many films per language (0 = all).
    #[arg(long, default_value_t = 0)]
    max_films: usize,
    /// Concurrent Modal predictions per film.
    #[arg(long, default_value_t = 8)]
    concurrency: usize,
    /// Output JSONL (appended to; already-present cues are skipped).
    #[arg(long, default_value = "out/phoneme-corpus-eval.jsonl")]
    out: PathBuf,
    /// Only print the summary of an existing output file; run nothing.
    #[arg(long, default_value_t = false)]
    summary_only: bool,
}

/// Matches the production partition in `audio_verification.rs`, so eval
/// predictions and any later production run share a cache.
const CACHE_VERSION: &str = "anchpop_lexide-pronunciation@953461d76eb5__greedy_v1";
const EXPECTED_DEPLOY_MARKER: &str = "953461d76eb5";

/// One evaluated cue, as a line of the output JSONL.
#[derive(Serialize, Deserialize)]
struct EvalRecord {
    imdb_id: String,
    title: String,
    lang: String,
    cue_index: usize,
    start_ms: i64,
    end_ms: i64,
    text: String,
    cleaned_text: String,
    label: CueLabel,
    /// Symmetric token WER between subtitle and transcript window.
    agreement_wer: f64,
    /// Symmetric WER between the cue and everything spoken in the clip span.
    #[serde(default)]
    exact_wer: f64,
    heard_text: String,
    /// An `[audio_event]` (music, laughter…) overlaps the cue span.
    audio_event_overlap: bool,
    /// Transcript speech within `NEIGHBOR_MARGIN_MS` outside the cue span.
    neighbor_speech: bool,
    /// Nested (not flattened): `ClipVerification` also carries a `text`
    /// field, and duplicate keys break the JSONL round-trip.
    verification: ClipVerification,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    if args.summary_only {
        print_summary(&args.out)?;
        return Ok(());
    }

    let wanted: Option<HashSet<String>> = args
        .langs
        .as_ref()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());

    let plan: Vec<PlanEntry> = serde_json::from_slice(
        &std::fs::read(args.corpus.join("plan.json")).context("reading plan.json")?,
    )?;

    // Resume: skip cues already evaluated.
    let mut done: HashSet<(String, usize)> = HashSet::new();
    if let Ok(existing) = std::fs::read_to_string(&args.out) {
        for line in existing.lines() {
            if let Ok(r) = serde_json::from_str::<EvalRecord>(line) {
                done.insert((r.imdb_id, r.cue_index));
            }
        }
    }
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args.out)?;

    // A hard per-request deadline: without one, a single hung Modal
    // connection wedges the whole buffered stream forever (observed: 40
    // minutes frozen mid-film). 120s comfortably covers a cold start; the
    // caller's retry loop handles the resulting timeout errors.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let empty_pronunciations: HashMap<String, language_utils::Pronunciations> = HashMap::new();

    let mut films_per_lang: HashMap<&'static str, usize> = HashMap::new();
    for entry in &plan {
        let Some(code) = course_code(&entry.original_language) else {
            continue;
        };
        if let Some(w) = &wanted
            && !w.contains(code)
        {
            continue;
        }
        let dir = args.corpus.join(&entry.imdb_id);
        let (srt, transcript_path, audio) = (
            dir.join("subtitle.srt"),
            dir.join("transcript.jsonl"),
            dir.join("audio.opus"),
        );
        if !(srt.exists() && transcript_path.exists() && audio.exists()) {
            continue;
        }
        if args.max_films > 0 && films_per_lang.get(code).copied().unwrap_or(0) >= args.max_films {
            continue;
        }
        *films_per_lang.entry(code).or_default() += 1;

        let language = Language::from_code(code).context("unreachable: unmapped course code")?;
        let cues = parse_cues(&std::fs::read_to_string(&srt)?);
        let transcript = load_transcript(&transcript_path)?;
        let candidates = label_cues(&cues, &transcript, Tokenization::Words);
        let count = |l: CueLabel| candidates.iter().filter(|c| c.label == l).count();
        let (n_pos, n_extra, n_mismatch, n_silent) = (
            count(CueLabel::Pos),
            count(CueLabel::NegExtraSpeech),
            count(CueLabel::NegMismatch),
            count(CueLabel::NegSilent),
        );
        if n_pos < MIN_FILM_POSITIVES {
            println!(
                "{} {} [{code}]: skipped — only {n_pos} verbatim positives ({} cues); subtitle \
                 likely desynced, a different cut, or forced-only",
                entry.imdb_id,
                entry.title,
                cues.len()
            );
            continue;
        }

        let picked: Vec<&Candidate> = sample(&candidates, CueLabel::Pos, args.per_film)
            .into_iter()
            .chain(sample(
                &candidates,
                CueLabel::NegExtraSpeech,
                args.per_film / 2,
            ))
            .chain(sample(
                &candidates,
                CueLabel::NegMismatch,
                args.per_film / 2,
            ))
            .chain(sample(&candidates, CueLabel::NegSilent, args.per_film / 2))
            .filter(|c| !done.contains(&(entry.imdb_id.clone(), c.cue_index)))
            .collect();

        println!(
            "{} {} [{code}]: {} cues → {n_pos} pos / {n_extra} extra-speech / {n_mismatch} mismatch / {n_silent} silent, {} to run",
            entry.imdb_id,
            entry.title,
            cues.len(),
            picked.len()
        );
        if picked.is_empty() {
            continue;
        }

        let ctx = VerifyContext::with_overrides(
            &http,
            &empty_pronunciations,
            language,
            CACHE_VERSION.to_string(),
            0.3,
            Some(EXPECTED_DEPLOY_MARKER.to_string()),
        )?;

        use futures::StreamExt;
        let results: Vec<Option<EvalRecord>> = futures::stream::iter(picked.into_iter().map(|c| {
            let ctx = &ctx;
            let audio = audio.clone();
            let entry_id = entry.imdb_id.clone();
            let entry_title = entry.title.clone();
            async move {
                let wav = match tokio::task::spawn_blocking({
                    let audio = audio.clone();
                    let (s, e) = (c.start_ms, c.end_ms);
                    move || slice_wav(&audio, s, e)
                })
                .await
                .expect("slice task panicked")
                {
                    Ok(w) => w,
                    Err(e) => {
                        eprintln!("  cue {}: {e:#}", c.cue_index);
                        return None;
                    }
                };
                // Empty pronunciation map ⇒ the espeak phrase-level rendering
                // is the sole expected sequence.
                let expected = expected_phoneme_variants(ctx, &c.cleaned_text, None);
                let verification = match verify_clip_bytes(
                    ctx,
                    &entry_id,
                    &c.cleaned_text,
                    &format!("{entry_id}#{}", c.cue_index),
                    &wav,
                    expected,
                )
                .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("  cue {}: {e:#}", c.cue_index);
                        return None;
                    }
                };
                Some(EvalRecord {
                    imdb_id: entry_id,
                    title: entry_title,
                    lang: code.to_string(),
                    cue_index: c.cue_index,
                    start_ms: c.start_ms,
                    end_ms: c.end_ms,
                    text: c.text.clone(),
                    cleaned_text: c.cleaned_text.clone(),
                    label: c.label,
                    agreement_wer: c.agreement_wer,
                    exact_wer: c.exact_wer,
                    heard_text: c.heard_text.clone(),
                    audio_event_overlap: c.audio_event_overlap,
                    neighbor_speech: c.neighbor_speech,
                    verification,
                })
            }
        }))
        .buffered(args.concurrency.max(1))
        .collect()
        .await;

        for record in results.into_iter().flatten() {
            writeln!(out_file, "{}", serde_json::to_string(&record)?)?;
        }
        out_file.flush()?;
    }

    print_summary(&args.out)
}

/// Distribution + separation + confusion summary over the output JSONL.
fn print_summary(out: &Path) -> Result<()> {
    let text = std::fs::read_to_string(out)
        .with_context(|| format!("no eval output at {}", out.display()))?;
    let records: Vec<EvalRecord> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    if records.is_empty() {
        println!("no records");
        return Ok(());
    }

    let mut by_lang: BTreeMap<&str, Vec<&EvalRecord>> = BTreeMap::new();
    for r in &records {
        by_lang.entry(r.lang.as_str()).or_default().push(r);
    }

    println!("\n=== phoneme-vs-espeak edit-distance % by label ===");
    println!(
        "{:<6} {:<14} {:>5}  {:>6} {:>6} {:>6} {:>6} {:>6}",
        "lang", "label", "n", "p10", "p25", "p50", "p75", "p90"
    );
    for (lang, rs) in &by_lang {
        for label in [
            CueLabel::Pos,
            CueLabel::NegExtraSpeech,
            CueLabel::NegMismatch,
            CueLabel::NegSilent,
        ] {
            let mut pcts: Vec<f64> = rs
                .iter()
                .filter(|r| r.label == label)
                .filter_map(|r| r.verification.edit_distance_pct)
                .collect();
            if pcts.is_empty() {
                continue;
            }
            pcts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let q = |p: f64| pcts[((pcts.len() - 1) as f64 * p) as usize];
            println!(
                "{:<6} {:<14} {:>5}  {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}%",
                lang,
                format!("{label:?}"),
                pcts.len(),
                q(0.10) * 100.0,
                q(0.25) * 100.0,
                q(0.50) * 100.0,
                q(0.75) * 100.0,
                q(0.90) * 100.0,
            );
        }
        // AUC of pct as a pos-vs-neg discriminator (rank-sum estimate):
        // probability a random negative scores higher than a random positive.
        let pos: Vec<f64> = rs
            .iter()
            .filter(|r| r.label == CueLabel::Pos)
            .filter_map(|r| r.verification.edit_distance_pct)
            .collect();
        let neg: Vec<f64> = rs
            .iter()
            .filter(|r| r.label != CueLabel::Pos)
            .filter_map(|r| r.verification.edit_distance_pct)
            .collect();
        if !pos.is_empty() && !neg.is_empty() {
            let mut wins = 0f64;
            for p in &pos {
                for n in &neg {
                    wins += if n > p {
                        1.0
                    } else if n == p {
                        0.5
                    } else {
                        0.0
                    };
                }
            }
            println!(
                "{lang:<6} AUC(neg>pos) = {:.3}  ({} pos vs {} neg)",
                wins / (pos.len() * neg.len()) as f64,
                pos.len(),
                neg.len()
            );
        }
    }

    println!("\n=== top substitutions among POSITIVES (systematic model↔espeak disagreement) ===");
    for (lang, rs) in &by_lang {
        let mut subs: HashMap<(String, String), usize> = HashMap::new();
        let mut extra: HashMap<String, usize> = HashMap::new();
        let mut missing: HashMap<String, usize> = HashMap::new();
        let mut total_ops = 0usize;
        for r in rs.iter().filter(|r| r.label == CueLabel::Pos) {
            for op in r.verification.alignment.iter().flatten() {
                total_ops += 1;
                match op {
                    AlignmentOp::Sub {
                        expected,
                        predicted,
                        ..
                    } => {
                        *subs
                            .entry((expected.clone(), predicted.clone()))
                            .or_default() += 1
                    }
                    AlignmentOp::Extra { predicted, .. } => {
                        *extra.entry(predicted.clone()).or_default() += 1
                    }
                    AlignmentOp::Missing { expected } => {
                        *missing.entry(expected.clone()).or_default() += 1
                    }
                    AlignmentOp::Match { .. } => {}
                }
            }
        }
        let mut subs: Vec<_> = subs.into_iter().collect();
        subs.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let mut extra: Vec<_> = extra.into_iter().collect();
        extra.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let mut missing: Vec<_> = missing.into_iter().collect();
        missing.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!("\n[{lang}] ({total_ops} alignment ops in positives)");
        print!("  subs (espeak→model): ");
        for ((e, p), n) in subs.iter().take(20) {
            print!("{e}→{p}:{n}  ");
        }
        print!("\n  model-extra: ");
        for (p, n) in extra.iter().take(12) {
            print!("{p}:{n}  ");
        }
        print!("\n  model-missing: ");
        for (e, n) in missing.iter().take(12) {
            print!("{e}:{n}  ");
        }
        println!();
    }
    Ok(())
}
