//! One-shot: compare the *current* wav2vec2 model against a saved
//! baseline run for the human voice actors.
//!
//! Reuses the production verifier path (`verify_clip` →
//! `VerifyContext`) so all the normalization, variant cross-product,
//! and threshold logic matches what the real pipeline does. The
//! Modal-side cache directory is partitioned by `WAV2VEC2_CACHE_VERSION`
//! in audio_verification.rs, so this binary writes to the *new* cache
//! dir while the old `audio_verification_all.jsonl` (committed to
//! `out/<lang>/`) carries the previous model's results untouched.
//!
//! Usage:
//!     cargo run --release --bin compare-audio-models -- <lang_dir>
//!
//! Example:
//!     cargo run --release --bin compare-audio-models -- generate-data/data/fra
//!
//! Output: a markdown table to stdout showing per-actor pass counts
//! (old vs new) plus the set of clips whose verdict flipped.

use anyhow::{Context, Result};
use generate_data::audio_verification::{
    ClipVerification, VerifyContext, expected_phoneme_variants, verify_clip,
};
use language_utils::{Pronunciations, Language};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct ManifestEntry {
    text: String,
    file: String,
    /// Optional speaker-realized IPA override. See main.rs for the full
    /// rationale; in short, replaces wikipron+espeak ground truth with a
    /// per-clip transcription when the speaker's actual production
    /// differs from citation form.
    #[serde(default)]
    phonemic_transcription: Option<String>,
}

fn load_word_to_pronunciation(out_dir: &Path) -> Result<HashMap<String, Pronunciations>> {
    let path = out_dir.join("word_to_pronunciation.jsonl");
    let f = File::open(&path).with_context(|| format!("open {}", path.display()))?;
    let mut map = HashMap::new();
    for line in BufReader::new(f).lines() {
        let line = line?;
        let (word, ap): (String, Pronunciations) =
            serde_json::from_str(&line).context("parse word_to_pronunciation entry")?;
        map.insert(word.to_lowercase(), ap);
    }
    Ok(map)
}

/// Trim a leading `./` so the old-log keys (which include it) compare
/// equal to paths we construct from `lang_data_dir.join(...)`.
fn norm_path(p: &str) -> String {
    p.strip_prefix("./").unwrap_or(p).to_string()
}

fn load_old_results(out_dir: &Path) -> Result<HashMap<String, ClipVerification>> {
    let path = out_dir.join("audio_verification_all.jsonl");
    let f = File::open(&path).with_context(|| format!("open {}", path.display()))?;
    let mut map = HashMap::new();
    for line in BufReader::new(f).lines() {
        let v: ClipVerification = serde_json::from_str(&line?)?;
        map.insert(norm_path(&v.wav_path), v);
    }
    Ok(map)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Init logging so warnings from the verifier path (notably the espeak
    // "phonemization failed" warning) actually reach stderr. Default level
    // is `warn` — quiet enough not to spam, loud enough to surface a
    // broken espeak environment that would otherwise silently strip
    // ground truth from contraction clips.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: compare-audio-models <lang_data_dir>");
        eprintln!("       e.g. generate-data/data/fra");
        std::process::exit(2);
    }
    let lang_data_dir = PathBuf::from(&args[1]);
    let lang_code = lang_data_dir
        .file_name()
        .and_then(|s| s.to_str())
        .context("lang_data_dir has no name")?;
    let language = match lang_code {
        "fra" => Language::French,
        "eng" => Language::English,
        "spa" => Language::Spanish,
        "deu" => Language::German,
        "ita" => Language::Italian,
        "por" => Language::Portuguese,
        "rus" => Language::Russian,
        "kor" => Language::Korean,
        other => anyhow::bail!("unsupported language code: {other}"),
    };
    let out_dir = PathBuf::from("out").join(lang_code);

    let word_to_pronunciation = load_word_to_pronunciation(&out_dir)?;
    let old_results = load_old_results(&out_dir)?;
    println!(
        "Loaded {} word pronunciations, {} old verification results",
        word_to_pronunciation.len(),
        old_results.len()
    );

    let cache_root = PathBuf::from(".cache");
    let http = reqwest::Client::new();
    let ctx = VerifyContext::new(&http, &cache_root, &word_to_pronunciation, language)?;

    let audio_root = lang_data_dir.join("audio");
    let mut contributors: Vec<_> = std::fs::read_dir(&audio_root)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    contributors.sort();

    // Per-actor verification summary:
    // (actor, old_pass, new_pass, both_pass, both_fail, flipped_to_pass, flipped_to_fail)
    type ActorSummary = (String, u32, u32, u32, u32, Vec<String>, Vec<String>);
    let mut summary: Vec<ActorSummary> = Vec::new();

    // Also dump per-clip new-model results so external scripts can diff two
    // runs head-to-head (e.g. unified vs unified-vad at threshold=0).
    let dump_path = std::env::var("PER_CLIP_DUMP_PATH").ok();
    let mut dump_file = match &dump_path {
        Some(p) => Some(File::create(p).with_context(|| format!("create {p}"))?),
        None => None,
    };
    use std::io::Write;

    for contrib in &contributors {
        let actor = contrib.file_name().unwrap().to_str().unwrap().to_string();
        let manifest_path = contrib.join("manifest.jsonl");
        if !manifest_path.exists() {
            continue;
        }
        let manifest_text = std::fs::read_to_string(&manifest_path)?;
        let entries: Vec<ManifestEntry> = manifest_text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;

        let mut old_pass = 0;
        let mut new_pass = 0;
        let mut both_pass = 0;
        let mut both_fail = 0;
        let mut to_pass = Vec::new();
        let mut to_fail = Vec::new();

        for entry in &entries {
            let wav_path = contrib.join(&entry.file);
            let wav_path_str = norm_path(&wav_path.to_string_lossy());
            let expected = expected_phoneme_variants(
                &ctx,
                &entry.text,
                entry.phonemic_transcription.as_deref(),
            );
            let new = verify_clip(&ctx, &actor, &entry.text, &wav_path, expected).await?;
            let new_ok = new.passed();
            if let Some(f) = dump_file.as_mut() {
                let row = serde_json::json!({
                    "wav": wav_path_str,
                    "actor": actor,
                    "text": entry.text,
                    "predicted": new.predicted_normalized,
                    "expected": new.expected,
                    "edit_distance": new.edit_distance,
                    "edit_distance_pct": new.edit_distance_pct,
                    "passed": new_ok,
                });
                writeln!(f, "{row}")?;
            }
            let Some(old) = old_results.get(&wav_path_str) else {
                // Clip not in baseline (e.g. b001–b200 were added after the
                // last verification run) — count as a "new" data point we
                // can't compare on; skip it.
                continue;
            };
            let old_ok = old.passed();

            if old_ok {
                old_pass += 1;
            }
            if new_ok {
                new_pass += 1;
            }
            match (old_ok, new_ok) {
                (true, true) => both_pass += 1,
                (false, false) => both_fail += 1,
                (false, true) => {
                    let old_pred = old.predicted_normalized.join(" ");
                    let new_pred = new.predicted_normalized.join(" ");
                    let exp = new
                        .expected
                        .as_ref()
                        .map(|e| e.join(" "))
                        .unwrap_or_default();
                    to_pass.push(format!(
                        "{}  text={:?}  exp={}  old={}  new={}",
                        entry.file, entry.text, exp, old_pred, new_pred
                    ));
                }
                (true, false) => {
                    let old_pred = old.predicted_normalized.join(" ");
                    let new_pred = new.predicted_normalized.join(" ");
                    let exp = new
                        .expected
                        .as_ref()
                        .map(|e| e.join(" "))
                        .unwrap_or_default();
                    let reason = new.failure_reason.as_deref().unwrap_or("");
                    to_fail.push(format!(
                        "{}  text={:?}  exp={}  old={}  new={}  reason={}",
                        entry.file, entry.text, exp, old_pred, new_pred, reason
                    ));
                }
            }
        }
        summary.push((
            actor, old_pass, new_pass, both_pass, both_fail, to_pass, to_fail,
        ));
    }

    println!("\n## Pass-count summary (per actor)\n");
    println!(
        "| actor | total | old pass | new pass | both pass | both fail | new→pass | new→fail |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for (actor, oldp, newp, bp, bf, tp, tf) in &summary {
        let total = bp + bf + tp.len() as u32 + tf.len() as u32;
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            actor,
            total,
            oldp,
            newp,
            bp,
            bf,
            tp.len(),
            tf.len()
        );
    }

    for (actor, _, _, _, _, tp, tf) in &summary {
        if !tp.is_empty() {
            println!("\n### {actor}: clips that now PASS (were failing)\n");
            for l in tp {
                println!("- {l}");
            }
        }
        if !tf.is_empty() {
            println!("\n### {actor}: clips that now FAIL (were passing)\n");
            for l in tf {
                println!("- {l}");
            }
        }
    }

    Ok(())
}
