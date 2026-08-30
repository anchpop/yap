//! Extract transcript-verified film clips as pronunciation training data.
//!
//! The selection is exactly the eval harness's verbatim gate
//! (`subtitle_corpus::label_cues`): a cue qualifies only when the full-film
//! ElevenLabs transcript confirms the subtitle was spoken verbatim with
//! nothing else inside the padded clip span and no audio-event overlap
//! (`CueLabel::Pos`). The phoneme model plays **no part** in selection — the
//! witness is independent, so training on these clips is not self-training.
//!
//! Every transcribed film is fair game — there is no held-out film split.
//! Regression detection is done by scoring BOTH the previous and the new
//! model on the same eval set (e.g. phoneme-corpus-eval on freshly
//! transcribed films), so training/eval overlap costs nothing there.
//!
//! Output is a pronunciation-corpus staging tree (NOT the live corpus — merge
//! deliberately after review):
//!
//!     <out-root>/<lang>/film_<imdb>_<cue>.wav
//!     <out-root>/<lang>/manifest.jsonl     (source: "film"; speaker_cluster
//!                                           from ElevenLabs diarization,
//!                                           per-film+chunk scoped)
//!
//! Training-side strictness beyond the eval's Pos gate: cues where a
//! neighboring word audibly bleeds into the ±150 ms padded span
//! (`edge_bleed`) are dropped by default — that word's onset/tail is IN the
//! clip but not in the label (`--allow-edge-bleed` to keep them).
//!
//! Usage (any host with the corpus mounted; no network, no model, no espeak):
//!     cargo run --release --bin subtitle-corpus-extract -- [--dry-run] \
//!         [--langs fra,jpn] [--per-film 120]

use anyhow::Result;
use clap::Parser;
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;
use std::path::PathBuf;
use subtitle_corpus::cues::{
    CueLabel, MIN_FILM_POSITIVES, course_code_full, label_cues, load_transcript, parse_cues,
    sample, slice_wav, tokenization_for,
};

#[derive(Parser, Debug)]
#[command(about = "Extract transcript-verified film cues as training clips")]
struct Args {
    /// Root of the subtitle corpus.
    #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
    corpus: PathBuf,
    /// Staging tree the clips + manifests are written into.
    #[arg(
        long,
        default_value = "/data/coding/lexide/pronunciation/data/film-staging"
    )]
    out_root: PathBuf,
    /// Comma-separated lang codes to include (default: all mapped).
    #[arg(long)]
    langs: Option<String>,
    /// Verbatim positives extracted per film (even-stride across the film).
    #[arg(long, default_value_t = 120)]
    per_film: usize,
    /// Keep cues whose padded span a neighboring word audibly bleeds into.
    #[arg(long, default_value_t = false)]
    allow_edge_bleed: bool,
    /// Report what would be extracted; write nothing.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let wanted: Option<HashSet<String>> = args
        .langs
        .as_ref()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());

    let plan = subtitle_corpus::library::read_plan(&args.corpus)?;
    // Resume: (imdb_id, cue_index) pairs already staged, across all langs.
    let mut done: HashSet<(String, usize)> = HashSet::new();
    if let Ok(dirs) = std::fs::read_dir(&args.out_root) {
        for dir in dirs.flatten() {
            let manifest = dir.path().join("manifest.jsonl");
            if let Ok(text) = std::fs::read_to_string(&manifest) {
                for line in text.lines() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
                        && let (Some(id), Some(cue)) = (
                            v.get("imdb_id").and_then(|x| x.as_str()),
                            v.get("cue_index").and_then(|x| x.as_u64()),
                        )
                    {
                        done.insert((id.to_string(), cue as usize));
                    }
                }
            }
        }
    }

    let mut totals: BTreeMap<&'static str, (usize, usize, usize)> = BTreeMap::new(); // films, clips, w/ speaker
    for entry in &plan {
        let Some(code) = course_code_full(&entry.original_language) else {
            continue;
        };
        if code == "kor" {
            continue; // no kor training config yet
        }
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

        let cues = parse_cues(&std::fs::read_to_string(&srt)?);
        let transcript = load_transcript(&transcript_path)?;
        let candidates = label_cues(&cues, &transcript, tokenization_for(code));
        let n_pos = candidates
            .iter()
            .filter(|c| c.label == CueLabel::Pos)
            .count();
        if n_pos < MIN_FILM_POSITIVES {
            println!(
                "{} {} [{code}]: skipped — only {n_pos} verbatim positives; likely desynced",
                entry.imdb_id, entry.title
            );
            continue;
        }

        let picked: Vec<_> = sample(&candidates, CueLabel::Pos, args.per_film)
            .into_iter()
            .filter(|c| args.allow_edge_bleed || !c.edge_bleed)
            .filter(|c| !done.contains(&(entry.imdb_id.clone(), c.cue_index)))
            .collect();
        let with_speaker = picked.iter().filter(|c| c.span_speaker.is_some()).count();
        println!(
            "{} {} [{code}]: {n_pos} pos → extracting {} ({} with speaker)",
            entry.imdb_id,
            entry.title,
            picked.len(),
            with_speaker
        );
        let t = totals.entry(code).or_default();
        t.0 += 1;
        t.1 += picked.len();
        t.2 += with_speaker;
        if args.dry_run || picked.is_empty() {
            continue;
        }

        let lang_dir = args.out_root.join(code);
        std::fs::create_dir_all(&lang_dir)?;
        let mut manifest = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(lang_dir.join("manifest.jsonl"))?;
        for c in picked {
            let wav = match slice_wav(&audio, c.start_ms, c.end_ms) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("  cue {}: {e:#}", c.cue_index);
                    continue;
                }
            };
            let file = format!("film_{}_{:05}.wav", entry.imdb_id, c.cue_index);
            std::fs::write(lang_dir.join(&file), &wav)?;
            // Schema matches data/audio manifests: preprocess resolves the
            // speaker as `speaker_cluster or voice`, so diarization goes in
            // speaker_cluster (film+chunk scoped: over-segmented, never
            // merged). espeak_voice stays null — for por that means the
            // pt-br default; European-Portuguese films need a manual
            // espeak_voice edit before merging, same as Pimsleur's split.
            let row = json!({
                "file": file,
                "sentence": c.cleaned_text,
                "source": "film",
                "voice": null,
                "espeak_voice": null,
                "speaker_cluster": c
                    .span_speaker
                    .as_ref()
                    .map(|s| format!("film:{}:{}", entry.imdb_id, s)),
                "imdb_id": entry.imdb_id,
                "title": entry.title,
                "cue_index": c.cue_index,
                "start_ms": c.start_ms,
                "end_ms": c.end_ms,
                "duration_sec": (c.end_ms - c.start_ms + 300) as f64 / 1000.0,
                "subtitle_text": c.text,
                "agreement_wer": c.agreement_wer,
                "exact_wer": c.exact_wer,
            });
            writeln!(manifest, "{row}")?;
        }
        manifest.flush()?;
    }

    println!("\n=== extraction totals ===");
    for (code, (films, clips, spk)) in &totals {
        println!("{code:9} {films:3} films  {clips:6} clips  {spk:6} with speaker");
    }
    if args.dry_run {
        println!("(dry run — nothing written)");
    }
    Ok(())
}
