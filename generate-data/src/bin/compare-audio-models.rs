//! Compare N wav2vec2 phoneme models against the human voice-actor clips.
//!
//! One self-contained Rust tool (formerly `scripts/compare_models.py` plus a
//! thinner version of this bin). For each model it resolves the HF ref to an
//! exact commit SHA; deploys the checkpoint to the *eval* Modal app
//! (`wav2vec2-phoneme-eval`, never the production app that generate-data /
//! yap-ai-backend hit); verifies the live container is the one just deployed
//! (exact deploy-marker match) before trusting any prediction; and runs every
//! clip through the production `verify_clip` path — reusing all the
//! normalization / variant / threshold logic — holding results in memory (no
//! subprocess, no temp round-trip). Then it prints a side-by-side pass-rate +
//! Phoneme-Error-Rate comparison. The FIRST model is the baseline the others
//! are diffed against; pass a single model to just evaluate it.
//!
//! Usage:
//!     cargo run --release --bin compare-audio-models -- \
//!         anchpop/model-a@<sha> anchpop/model-b [--actor Jean-Cavard]
//!
//! The only external process is the `modal` CLI for deploy/stop (there is no
//! Rust Modal SDK); everything else is native.

use anyhow::{Context, Result, bail};
use clap::Parser;
use generate_data::audio_verification::{
    ClipVerification, VerifyContext, expected_phoneme_variants, normalize_phoneme, verify_clip,
};
use language_utils::{Language, Pronunciations};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Dedicated eval app — NEVER the production "wav2vec2-phoneme". The Modal file
/// reads `WAV2VEC2_APP_NAME`, so deploying with this isolates eval runs.
const EVAL_APP: &str = "wav2vec2-phoneme-eval";
const MODAL_PY: &str = "modal-envs/wav2vec2_phoneme.py";
/// At threshold 0 a clip passes only on an exact phoneme match — the right
/// setting for head-to-head model comparison (see the project audio-verify
/// threshold note).
const COMPARE_THRESHOLD: f64 = 0.0;
/// U+0303 COMBINING TILDE — marks a nasal vowel.
const NASAL: char = '\u{0303}';

fn eval_endpoint_url() -> String {
    format!("https://anchpop--{EVAL_APP}-wav2vec2phoneme-predict.modal.run")
}

#[derive(Parser)]
#[command(
    about = "Compare N wav2vec2 phoneme models against the voice-actor clips.",
    long_about = None
)]
struct Args {
    /// Model specs (`repo` or `repo@ref`). The FIRST is the baseline the others
    /// are diffed against. Pass a single model to just evaluate it.
    #[arg(required = true)]
    models: Vec<String>,
    /// Language data dir; its basename is the ISO code (e.g. `fra`).
    #[arg(long, default_value = "generate-data/data/fra")]
    lang: PathBuf,
    /// Restrict to a single actor (e.g. `Jean-Cavard`).
    #[arg(long)]
    actor: Option<String>,
    /// Reuse cached predictions; skip Modal deploy entirely.
    #[arg(long)]
    skip_deploy: bool,
}

// ─────────────────────────── model spec / naming ───────────────────────────

fn parse_model(spec: &str) -> (String, String) {
    match spec.split_once('@') {
        Some((id, rev)) => (id.to_string(), rev.to_string()),
        None => (spec.to_string(), "main".to_string()),
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Filename-safe slug; keeps the historical `/` → `__` mapping for HF org/name.
/// Revisions are always resolved SHAs here, so the SHA suffix is always added.
fn safe_name(model_id: &str, revision: &str) -> String {
    let base = sanitize(&model_id.replace('/', "__"));
    if revision == "main" {
        base
    } else {
        let short: String = revision.chars().take(12).collect();
        format!("{base}__{}", sanitize(&short))
    }
}

fn label_for(model_id: &str, revision: &str) -> String {
    let name = model_id.rsplit('/').next().unwrap_or(model_id);
    let short: String = revision.chars().take(8).collect();
    format!("{name}@{short}")
}

// ─────────────────────────── HF ref resolution ─────────────────────────────

/// Resolve a possibly-mutable HF ref (e.g. `main`) to an exact commit SHA, so
/// the stable per-model cache key tracks the actual weights — a later
/// force-push to the same ref can't reuse stale cached predictions.
async fn resolve_revision(
    http: &reqwest::Client,
    model_id: &str,
    revision: &str,
) -> Result<String> {
    let url = format!("https://huggingface.co/api/models/{model_id}/revision/{revision}");
    let mut req = http.get(&url).header("User-Agent", "compare-audio-models");
    if let Ok(token) = std::env::var("HF_TOKEN") {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .with_context(|| format!("HF API request failed for {model_id}@{revision}"))?
        .error_for_status()
        .with_context(|| format!("HF API error for {model_id}@{revision}"))?;
    let body: serde_json::Value = resp.json().await.context("parse HF API response")?;
    body.get("sha")
        .and_then(|s| s.as_str())
        .map(String::from)
        .with_context(|| format!("no `sha` in HF response for {model_id}@{revision}"))
}

// ─────────────────────────── Modal deploy plumbing ─────────────────────────

/// Best-effort: drain warm containers before a redeploy. `-y` keeps it
/// non-interactive; failures (e.g. nothing running) are ignored.
fn stop_app() {
    let _ = Command::new("modal")
        .args(["app", "stop", EVAL_APP, "-y"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Deploy the tracked Modal file to the eval app, selecting the checkpoint
/// entirely through the environment — `wav2vec2_phoneme.py` reads all four of
/// these, so the file is never rewritten and eval and production run byte-identical
/// code. `deploy_marker` is unique per (re)deploy: it feeds both the freshness
/// check and, via MODEL_WEIGHTS_VERSION, the image cache-buster that forces a
/// fresh image + container. It is deliberately NOT the prediction cache key —
/// that stays stable per model.
fn deploy(model_id: &str, revision: &str, deploy_marker: &str) -> Result<()> {
    let out = Command::new("modal")
        .arg("deploy")
        .arg(MODAL_PY)
        .env("WAV2VEC2_APP_NAME", EVAL_APP)
        .env("WAV2VEC2_MODEL_ID", model_id)
        .env("WAV2VEC2_MODEL_REVISION", revision)
        .env("WAV2VEC2_DEPLOY_MARKER", deploy_marker)
        .output()
        .context("failed to run `modal deploy` (is the modal CLI installed?)")?;
    if !out.status.success() {
        eprint!("{}", String::from_utf8_lossy(&out.stdout));
        eprint!("{}", String::from_utf8_lossy(&out.stderr));
        bail!("modal deploy failed ({})", out.status);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find(|l| l.contains("App deployed"))
        .unwrap_or("deploy ok");
    println!("  → {}", line.trim());
    Ok(())
}

fn fresh_marker(sn: &str, seq: &mut u64) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    *seq += 1;
    format!("{sn}__{ts}_{seq}")
}

/// One marker_only probe. Returns the parsed JSON (`deploy_marker` on success,
/// `load_error` if the model failed to load), or None on a transport error
/// (container still cold — the 300s timeout lets it serve while starting).
async fn probe_endpoint(http: &reqwest::Client, url: &str) -> Option<serde_json::Value> {
    let resp = http
        .post(url)
        .json(&serde_json::json!({ "marker_only": true }))
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .ok()?;
    resp.json::<serde_json::Value>().await.ok()
}

/// Deploy `model_id@revision` to the eval app and return the verified deploy
/// marker once the live container reports it. Bumps the marker + redeploys on a
/// stale-container mismatch (the warm-container contamination bug); aborts with
/// the real traceback if the model failed to load.
async fn deploy_and_verify(
    http: &reqwest::Client,
    url: &str,
    sn: &str,
    model_id: &str,
    revision: &str,
    seq: &mut u64,
) -> Result<String> {
    let deploy_marker = |marker: &str| -> Result<()> {
        stop_app();
        deploy(model_id, revision, marker)
    };

    let mut marker = fresh_marker(sn, seq);
    deploy_marker(&marker)?;
    for attempt in 1..=6 {
        match probe_endpoint(http, url).await {
            None => {
                println!("  → marker probe {attempt}: no response yet, retrying ...");
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            Some(resp) => {
                if let Some(err) = resp.get("load_error").and_then(|e| e.as_str()) {
                    bail!("Modal container started but the model failed to load:\n\n{err}");
                }
                let live = resp.get("deploy_marker").and_then(|m| m.as_str());
                if live == Some(marker.as_str()) {
                    println!("  → deploy marker verified ✓ ({marker})");
                    return Ok(marker);
                }
                println!(
                    "  → marker {attempt}/6 MISMATCH (live={live:?}, want={marker}) — \
                     stale container, forcing fresh redeploy ..."
                );
                marker = fresh_marker(sn, seq);
                deploy_marker(&marker)?;
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        }
    }
    bail!(
        "deploy marker never verified after 6 redeploys — likely the Modal \
         warm-container bug; aborting rather than reporting contaminated results"
    )
}

// ─────────────────────────── data loading / verification ───────────────────

#[derive(serde::Deserialize)]
struct ManifestEntry {
    text: String,
    file: String,
    /// Optional speaker-realized IPA override (connected-speech form that may
    /// differ from citation form), threaded through to the verifier.
    #[serde(default)]
    phonemic_transcription: Option<String>,
}

fn load_word_to_pronunciation(out_dir: &Path) -> Result<HashMap<String, Pronunciations>> {
    let path = out_dir.join("word_to_pronunciation.jsonl");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("open {}", path.display()))?;
    let mut map = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (word, pron): (String, Pronunciations) =
            serde_json::from_str(line).context("parse word_to_pronunciation entry")?;
        map.insert(word.to_lowercase(), pron);
    }
    Ok(map)
}

fn language_from_dir(lang_dir: &Path) -> Result<(Language, String)> {
    let code = lang_dir
        .file_name()
        .and_then(|s| s.to_str())
        .context("lang dir has no name")?
        .to_string();
    let language = match code.as_str() {
        "fra" => Language::French,
        "eng" => Language::English,
        "spa" => Language::Spanish,
        "deu" => Language::German,
        "ita" => Language::Italian,
        "por" => Language::Portuguese,
        "rus" => Language::Russian,
        "kor" => Language::Korean,
        other => bail!("unsupported language code: {other}"),
    };
    Ok((language, code))
}

fn actor_dirs(audio_root: &Path, actor_filter: &Option<String>) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(audio_root)
        .with_context(|| format!("read {}", audio_root.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter(
            |p| match (actor_filter, p.file_name().and_then(|n| n.to_str())) {
                (Some(want), Some(name)) => want == name,
                (Some(_), None) => false,
                (None, _) => true,
            },
        )
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// Verify every clip for one model, in-process, holding results in memory.
async fn verify_model(
    http: &reqwest::Client,
    cache_root: &Path,
    word_to_pronunciation: &HashMap<String, Pronunciations>,
    language: Language,
    cache_key: String,
    expected_marker: Option<String>,
    contributors: &[PathBuf],
) -> Result<Vec<ClipVerification>> {
    let ctx = VerifyContext::with_overrides(
        http,
        cache_root,
        word_to_pronunciation,
        language,
        cache_key,
        COMPARE_THRESHOLD,
        expected_marker,
    )?;
    let mut results = Vec::new();
    for contrib in contributors {
        let actor = contrib
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let manifest = contrib.join("manifest.jsonl");
        if !manifest.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest)
            .with_context(|| format!("read {}", manifest.display()))?;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let entry: ManifestEntry =
                serde_json::from_str(line).context("parse manifest entry")?;
            let wav_path = contrib.join(&entry.file);
            let expected = expected_phoneme_variants(
                &ctx,
                &entry.text,
                entry.phonemic_transcription.as_deref(),
            );
            let v = verify_clip(&ctx, &actor, &entry.text, &wav_path, expected).await?;
            results.push(v);
        }
    }
    Ok(results)
}

// ─────────────────────────── diff / categorization ─────────────────────────

struct ModelRun {
    label: String,
    results: Vec<ClipVerification>,
}

impl ModelRun {
    fn by_wav(&self) -> HashMap<&str, &ClipVerification> {
        self.results
            .iter()
            .map(|v| (v.wav_path.as_str(), v))
            .collect()
    }
}

fn norm_seq(tokens: &[String], language: Language) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|t| normalize_phoneme(t, language))
        .collect()
}

/// Phoneme Error Rate over `keys`: Σ edit_distance / Σ |expected|, counting
/// only scoreable clips (non-empty expected + an actual alignment). Returns
/// (per_pct, edits, ref_len).
fn per_stats(
    by_wav: &HashMap<&str, &ClipVerification>,
    keys: &BTreeSet<&str>,
    actor: Option<&str>,
) -> (f64, usize, usize) {
    let mut edits = 0usize;
    let mut ref_len = 0usize;
    for k in keys {
        let Some(v) = by_wav.get(k) else { continue };
        if let Some(a) = actor
            && v.actor != a
        {
            continue;
        }
        if let (Some(exp), Some(ed)) = (&v.expected, v.edit_distance)
            && !exp.is_empty()
        {
            edits += ed;
            ref_len += exp.len();
        }
    }
    let pct = if ref_len > 0 {
        100.0 * edits as f64 / ref_len as f64
    } else {
        0.0
    };
    (pct, edits, ref_len)
}

fn passed(by_wav: &HashMap<&str, &ClipVerification>, k: &str) -> bool {
    by_wav.get(k).is_some_and(|v| v.passed())
}

/// Bucket a (expected, predicted) pair — both already normalized — into a
/// failure category. Mirrors the cheat-sheet developed analyzing the clips.
fn categorize_failure(exp: &[String], pred: &[String]) -> &'static str {
    if pred.is_empty() {
        return "audio_defect";
    }
    if exp.is_empty() {
        return "other";
    }
    if exp.len() > 6 {
        return "phrase_multi";
    }
    if exp.iter().any(|t| t.contains(NASAL)) {
        return "nasal_vowel";
    }
    if exp.iter().any(|t| t == "ə") {
        // schwa → ø, or schwa dropped, possibly only at one position.
        let rounded: Vec<String> = exp
            .iter()
            .map(|t| {
                if t == "ə" {
                    "ø".to_string()
                } else {
                    t.clone()
                }
            })
            .collect();
        if rounded == pred {
            return "schwa_to_o";
        }
        let dropped: Vec<String> = exp.iter().filter(|t| *t != "ə").cloned().collect();
        if dropped == pred {
            return "schwa_to_o";
        }
        for keep in 0..exp.len() {
            let test: Vec<String> = exp
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    if t == "ə" && i == keep {
                        "ø".to_string()
                    } else {
                        t.clone()
                    }
                })
                .collect();
            if test == pred {
                return "schwa_to_o";
            }
            let test_dropped: Vec<String> = test
                .iter()
                .enumerate()
                .filter(|(i, t)| !(*i != keep && *t == "ə"))
                .map(|(_, t)| t.clone())
                .collect();
            if test_dropped == pred {
                return "schwa_to_o";
            }
        }
    }
    if exp.len() == pred.len() {
        let diffs: Vec<(&String, &String)> = exp.iter().zip(pred).filter(|(e, p)| e != p).collect();
        if diffs.len() == 1 {
            let (e, p) = diffs[0];
            let pair = |a: &str, b: &str| (e == a && p == b) || (e == b && p == a);
            if pair("e", "ɛ") {
                return "e_vs_ɛ";
            }
            if pair("a", "ɑ") {
                return "a_vs_ɑ";
            }
            if e == "ʁ" && matches!(p.as_str(), "x" | "ɾ" | "r") {
                return "ʁ_variant";
            }
        }
    }
    let r_variants = ["x", "ɾ", "r"];
    if exp.iter().any(|t| t == "ʁ")
        && pred
            .iter()
            .any(|p| r_variants.contains(&p.as_str()) && !exp.iter().any(|e| e == p))
    {
        return "ʁ_variant";
    }
    if pred.len() > exp.len() {
        return "extra_phonemes";
    }
    if pred.len() < exp.len() {
        return "missing_phoneme";
    }
    "other"
}

// ─────────────────────────── rendering ─────────────────────────────────────

fn render_summary(
    runs: &[ModelRun],
    maps: &[HashMap<&str, &ClipVerification>],
    shared: &BTreeSet<&str>,
) {
    let n = shared.len();
    println!("\n# {}-model comparison ({n} shared clips)", runs.len());

    // Pass + PER, ranked by PER (lower better), pass count breaks ties.
    let mut rows: Vec<(usize, usize, f64)> = (0..runs.len())
        .map(|i| {
            let pass = shared.iter().filter(|k| passed(&maps[i], k)).count();
            let (per, ..) = per_stats(&maps[i], shared, None);
            (i, pass, per)
        })
        .collect();
    rows.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap().then(b.1.cmp(&a.1)));

    println!("\n## Pass + PER @ threshold=0 (ranked by PER)\n");
    println!("| rank | model | pass | pct | PER |");
    println!("|---|---|---|---|---|");
    for (rank, (i, pass, per)) in rows.iter().enumerate() {
        let pct = if n > 0 {
            100.0 * *pass as f64 / n as f64
        } else {
            0.0
        };
        println!(
            "| {} | {} | **{pass}** | {pct:.1}% | **{per:.2}%** |",
            rank + 1,
            runs[*i].label
        );
    }

    // Per-actor PER (only when >1 actor present).
    let actors: BTreeSet<&str> = shared
        .iter()
        .filter_map(|k| maps[0].get(k).map(|v| v.actor.as_str()))
        .collect();
    if actors.len() > 1 {
        println!("\n## Per-actor PER\n");
        print!("| actor |");
        for r in runs {
            print!(" {} |", r.label);
        }
        println!();
        println!("|---{}", "|---".repeat(runs.len()) + "|");
        for ac in &actors {
            print!("| {ac} |");
            for m in maps {
                let (per, ..) = per_stats(m, shared, Some(ac));
                print!(" {per:.2}% |");
            }
            println!();
        }
    }

    // Nasal-vowel cohort (expected contains a nasal vowel).
    let nasal: BTreeSet<&str> = shared
        .iter()
        .filter(|k| {
            maps[0]
                .get(*k)
                .and_then(|v| v.expected.as_ref())
                .is_some_and(|e| e.iter().any(|t| t.contains(NASAL)))
        })
        .copied()
        .collect();
    if !nasal.is_empty() {
        println!("\n## Nasal-vowel clips ({} of {n})\n", nasal.len());
        println!("| model | pass | pct | PER |");
        println!("|---|---|---|---|");
        for (i, run) in runs.iter().enumerate() {
            let pass = nasal.iter().filter(|k| passed(&maps[i], k)).count();
            let (per, ..) = per_stats(&maps[i], &nasal, None);
            let pct = 100.0 * pass as f64 / nasal.len() as f64;
            println!("| {} | {pass} | {pct:.0}% | {per:.2}% |", run.label);
        }
    }
}

/// Per-challenger diff against the baseline: headline deltas + categorized
/// regressions / recoveries / shared failures.
fn render_pairwise(
    base: &ModelRun,
    chal: &ModelRun,
    base_map: &HashMap<&str, &ClipVerification>,
    chal_map: &HashMap<&str, &ClipVerification>,
    shared: &BTreeSet<&str>,
    language: Language,
) {
    let both_pass = shared
        .iter()
        .filter(|k| passed(base_map, k) && passed(chal_map, k))
        .count();
    let both_fail = shared
        .iter()
        .filter(|k| !passed(base_map, k) && !passed(chal_map, k))
        .count();
    let regressions: Vec<&str> = shared
        .iter()
        .filter(|k| passed(base_map, k) && !passed(chal_map, k))
        .copied()
        .collect();
    let recoveries: Vec<&str> = shared
        .iter()
        .filter(|k| !passed(base_map, k) && passed(chal_map, k))
        .copied()
        .collect();
    let base_pass = both_pass + regressions.len();
    let chal_pass = both_pass + recoveries.len();
    let (per_b, ..) = per_stats(base_map, shared, None);
    let (per_c, ..) = per_stats(chal_map, shared, None);

    println!("\n# {} (baseline)  vs  {}", base.label, chal.label);
    println!("\n| metric | {} | {} |", base.label, chal.label);
    println!("|---|---|---|");
    let pct = |p: usize| 100.0 * p as f64 / shared.len() as f64;
    println!(
        "| pass @ threshold=0 | **{base_pass}** ({:.1}%) | **{chal_pass}** ({:.1}%) |",
        pct(base_pass),
        pct(chal_pass)
    );
    println!("| **PER** | **{per_b:.2}%** | **{per_c:.2}%** |");
    println!("| both pass | {both_pass} | |");
    println!("| both fail | {both_fail} | |");
    println!(
        "| **net pass (chal − base)** | | **{:+}** |",
        chal_pass as i64 - base_pass as i64
    );
    println!(
        "| **net PER (chal − base)** | | **{:+.2}pp** |",
        per_c - per_b
    );

    render_category(
        &format!("{} REGRESSIONS (baseline ✓, challenger ✗)", chal.label),
        &regressions,
        chal_map,
        base_map,
        language,
    );
    render_category(
        &format!("{} RECOVERIES (baseline ✗, challenger ✓)", chal.label),
        &recoveries,
        base_map,
        chal_map,
        language,
    );

    if both_fail > 0 {
        let mut buckets: HashMap<&'static str, usize> = HashMap::new();
        for k in shared {
            if passed(base_map, k) || passed(chal_map, k) {
                continue;
            }
            if let Some(v) = chal_map.get(k) {
                let exp = norm_seq(v.expected.as_deref().unwrap_or(&[]), language);
                let pred = norm_seq(&v.predicted_normalized, language);
                *buckets.entry(categorize_failure(&exp, &pred)).or_default() += 1;
            }
        }
        println!("\n## Shared failures (both fail) — {both_fail} clips\n");
        println!("| category | count |");
        println!("|---|---|");
        let mut sorted: Vec<_> = buckets.into_iter().collect();
        sorted.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (cat, n) in sorted {
            println!("| {cat} | {n} |");
        }
    }
}

/// Group flipped clips by failure category and print per-category tables.
/// `loser_map` predicted the wrong thing; `winner_map` got it right.
fn render_category(
    title: &str,
    clips: &[&str],
    loser_map: &HashMap<&str, &ClipVerification>,
    winner_map: &HashMap<&str, &ClipVerification>,
    language: Language,
) {
    if clips.is_empty() {
        return;
    }
    struct Row {
        clip: String,
        text: String,
        exp: String,
        loser: String,
        winner: String,
    }
    let mut buckets: HashMap<&'static str, Vec<Row>> = HashMap::new();
    for c in clips {
        let (Some(lv), Some(wv)) = (loser_map.get(c), winner_map.get(c)) else {
            continue;
        };
        let exp = norm_seq(lv.expected.as_deref().unwrap_or(&[]), language);
        let loser = norm_seq(&lv.predicted_normalized, language);
        let winner = norm_seq(&wv.predicted_normalized, language);
        let cat = categorize_failure(&exp, &loser);
        let short = short_clip(c);
        buckets.entry(cat).or_default().push(Row {
            clip: short,
            text: lv.text.clone(),
            exp: exp.join(" "),
            loser: loser.join(" "),
            winner: winner.join(" "),
        });
    }
    println!("\n## {title} — {} clips, by category\n", clips.len());
    let mut sorted: Vec<_> = buckets.into_iter().collect();
    sorted.sort_by_key(|(_, rows)| std::cmp::Reverse(rows.len()));
    for (cat, rows) in sorted {
        println!("### {cat} ({})\n", rows.len());
        println!("| clip | text | expected | loser | winner |");
        println!("|---|---|---|---|---|");
        for r in rows {
            println!(
                "| `{}` | `{}` | `{}` | `{}` | `{}` |",
                r.clip, r.text, r.exp, r.loser, r.winner
            );
        }
        println!();
    }
}

/// `<actor>/<file>` from a full wav path, for compact table labels.
fn short_clip(path: &str) -> String {
    let parts: Vec<&str> = path.rsplit('/').take(2).collect();
    match parts.as_slice() {
        [file, actor] => format!("{actor}/{file}"),
        _ => path.to_string(),
    }
}

// ─────────────────────────── main ──────────────────────────────────────────

async fn run(args: Args) -> Result<()> {
    let (language, code) = language_from_dir(&args.lang)?;
    let out_dir = PathBuf::from("out").join(&code);
    let word_to_pronunciation = load_word_to_pronunciation(&out_dir)?;
    println!(
        "Loaded {} word pronunciations for {code}",
        word_to_pronunciation.len()
    );

    let audio_root = args.lang.join("audio");
    let contributors = actor_dirs(&audio_root, &args.actor)?;
    if contributors.is_empty() {
        bail!("no actor directories under {}", audio_root.display());
    }

    // --skip-deploy means "reuse cached predictions only": force cache-only so a
    // cache miss errors out instead of silently calling whatever the eval app
    // currently serves (unverified) and caching it under this model's key.
    if args.skip_deploy {
        generate_data::set_cache_only(true);
    }

    let cache_root = PathBuf::from(".cache");
    let http = reqwest::Client::new();
    let url = eval_endpoint_url();
    let mut seq: u64 = 0;

    let mut runs: Vec<ModelRun> = Vec::new();
    for spec in &args.models {
        let (model_id, ref_) = parse_model(spec);
        let revision = resolve_revision(&http, &model_id, &ref_).await?;
        let sn = safe_name(&model_id, &revision);
        let label = label_for(&model_id, &revision);
        println!("\n### {model_id} (revision={revision})");

        let expected_marker = if args.skip_deploy {
            None
        } else {
            Some(deploy_and_verify(&http, &url, &sn, &model_id, &revision, &mut seq).await?)
        };

        let cache_key = format!("{sn}__greedy_v1");
        println!("  → verifying clips (cache={cache_key})");
        let results = verify_model(
            &http,
            &cache_root,
            &word_to_pronunciation,
            language,
            cache_key,
            expected_marker,
            &contributors,
        )
        .await?;
        runs.push(ModelRun { label, results });
    }

    // Shared clip set across all models (every model verifies the same set,
    // but intersect defensively).
    let maps: Vec<HashMap<&str, &ClipVerification>> = runs.iter().map(|r| r.by_wav()).collect();
    let mut shared: BTreeSet<&str> = maps[0].keys().copied().collect();
    for m in &maps[1..] {
        let ks: BTreeSet<&str> = m.keys().copied().collect();
        shared = shared.intersection(&ks).copied().collect();
    }
    if shared.is_empty() {
        bail!("no shared clips across models");
    }

    render_summary(&runs, &maps, &shared);
    for i in 1..runs.len() {
        render_pairwise(&runs[0], &runs[i], &maps[0], &maps[i], &shared, language);
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    // Point the in-process verifier at the EVAL app, never production. Set
    // before the async runtime starts (single-threaded here, so sound) so the
    // lazily-read endpoint global picks it up.
    unsafe {
        std::env::set_var("WAV2VEC2_ENDPOINT_URL", eval_endpoint_url());
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // Deploys go through a temp copy of the Modal file (see deploy_and_verify),
    // so the tracked production source is never touched — no restore needed.
    tokio::runtime::Runtime::new()
        .context("build tokio runtime")
        .and_then(|rt| rt.block_on(run(args)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parse_and_safe_name() {
        assert_eq!(parse_model("a/b"), ("a/b".into(), "main".into()));
        assert_eq!(parse_model("a/b@abc"), ("a/b".into(), "abc".into()));
        assert_eq!(safe_name("anchpop/foo", "main"), "anchpop__foo");
        assert_eq!(
            safe_name("anchpop/foo", "00a661934cdd1179"),
            "anchpop__foo__00a661934cdd"
        );
    }

    #[test]
    fn categorize_buckets() {
        assert_eq!(categorize_failure(&v(&["a"]), &[]), "audio_defect");
        assert_eq!(categorize_failure(&v(&["ɑ̃"]), &v(&["a"])), "nasal_vowel");
        assert_eq!(
            categorize_failure(&v(&["s", "ə"]), &v(&["s"])),
            "schwa_to_o"
        );
        assert_eq!(
            categorize_failure(&v(&["d", "ə"]), &v(&["d", "ø"])),
            "schwa_to_o"
        );
        assert_eq!(
            categorize_failure(&v(&["m", "e"]), &v(&["m", "ɛ"])),
            "e_vs_ɛ"
        );
        assert_eq!(
            categorize_failure(&v(&["t", "a"]), &v(&["t", "a", "k"])),
            "extra_phonemes"
        );
        assert_eq!(
            categorize_failure(&v(&["t", "a", "k"]), &v(&["t", "a"])),
            "missing_phoneme"
        );
        // >6 tokens → phrase_multi regardless of content
        assert_eq!(
            categorize_failure(&v(&["a", "b", "c", "d", "e", "f", "g"]), &v(&["a"])),
            "phrase_multi"
        );
    }

    #[test]
    fn short_clip_label() {
        assert_eq!(
            short_clip("generate-data/data/fra/audio/Jean-Cavard/21a_pain.wav"),
            "Jean-Cavard/21a_pain.wav"
        );
    }

    /// `deploy` selects the checkpoint purely through the environment, which
    /// only works while the Modal file still reads those variables. If someone
    /// hard-codes one back, eval would quietly deploy production's pin — so
    /// assert the contract against the tracked source. (The runtime marker check
    /// would also catch it, but only after a full deploy.)
    #[test]
    fn modal_file_honors_the_env_overrides_deploy_sets() {
        let src = std::fs::read_to_string(format!("../{MODAL_PY}"))
            .or_else(|_| std::fs::read_to_string(MODAL_PY))
            .expect("read the tracked Modal source");
        // Whitespace-stripped so a black-style line wrap inside the call still
        // matches — we're asserting the read exists, not how it's formatted.
        let packed: String = src.chars().filter(|c| !c.is_whitespace()).collect();
        for var in [
            "WAV2VEC2_APP_NAME",
            "WAV2VEC2_MODEL_ID",
            "WAV2VEC2_MODEL_REVISION",
            "WAV2VEC2_DEPLOY_MARKER",
        ] {
            assert!(
                packed.contains(&format!("os.environ.get(\"{var}\"")),
                "{MODAL_PY} no longer reads {var}; `deploy` sets it and would be ignored"
            );
        }
    }
}
