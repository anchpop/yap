//! Serve-ready clip export: every passing clip becomes a directory of
//! `hi.mp4` + `lo.mp4` + `meta.json`, cut generously from the source film
//! (neighboring subtitle lines as context) with the sidecar — not the file
//! boundary — defining what the clip *is*. Schema and rationale:
//! `docs/clip-sidecar.md`.
//!
//! Everything in the sidecar comes from artifacts already on disk; the
//! forced alignment re-reads the cached frame matrices under
//! [`phoneme_verify::set_cache_only`], so an export run spends no inference.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{bail, Context, Result};
use language_utils::Language;
use movie_subtitles::cleanup_subtitle_text;
use movie_subtitles::segment::SubtitleSegmenter;
use phoneme_verify::VerifyContext;
use serde_json::json;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::clips::{clips_path, read_clips, subtitle_sentences, Clip, Provenance};
use crate::cues::{load_transcript, parse_cues, repair_latin_homoglyphs, slice_wav_padded};
use crate::library::{course_dir, read_plan, truncate, Movie};
use crate::sync::Cue;
use crate::transcript::{Kind, Spoken};
use movie_subtitles::sentences::KeyedSentence;

/// Loudness target for the served clips, measured over the critical span.
const TARGET_I: f64 = -18.0;
/// True-peak ceiling; caps the gain on quiet-but-peaky clips.
const TP_CEIL: f64 = -1.5;
/// A neighboring subtitle line joins the cut only when the silence between
/// it and the clip is at most this — a longer gap usually means a scene
/// change, and unrelated footage is worse than no context.
const CTX_GAP_MS: i64 = 2_000;
/// The cut never exceeds this; context is dropped (furthest line first)
/// before the scored span ever is.
const CTX_CAP_MS: i64 = 15_000;
/// Breathing room past a context line's cue stamps.
const CTX_PAD_MS: i64 = 150;
/// The hi rendition is never upscaled and never taller than this.
const MAX_HEIGHT: i64 = 1440;
const LO_HEIGHT: i64 = 480;
const HI_CRF: u32 = 19;
const HI_PRESET: &str = "medium";
const HI_AAC: &str = "160k";
const LO_CRF: u32 = 27;
const LO_PRESET: &str = "veryfast";
const LO_AAC: &str = "96k";

/// Sidecar `format` field.
const SIDECAR_FORMAT: u32 = 2;

/// Everything that shapes the rendered files, in one comparable string.
/// Built from the constants so no tweak can be forgotten; anything that
/// changes the output must appear here or in the per-film stamp.
fn encode_recipe() -> String {
    format!(
        "hi h264 crf{HI_CRF} {HI_PRESET} aac{HI_AAC} le{MAX_HEIGHT}p | \
         lo crf{LO_CRF} {LO_PRESET} aac{LO_AAC} le{LO_HEIGHT}p | \
         loudnorm I{TARGET_I} TP{TP_CEIL} critical linear | \
         ctx gap{CTX_GAP_MS} cap{CTX_CAP_MS} pad{CTX_PAD_MS} | \
         keyframe@critical | tonemap zscale hable bt709 | lanczos yuv420p"
    )
}

/// The provenance of one exported clip directory: a pure function of every
/// input that could change its bytes. Written into the sidecar as `export`;
/// a clip is skipped on resume only when its stored stamp equals the one
/// computed now — anything else (missing, older format, different recipe,
/// re-mapped clips, replaced video) is deleted and re-rendered. Better to
/// recalculate than to trust a cache whose inputs may have moved.
fn export_stamp(provenance: &Provenance, movie: &Movie, audio_stream: u32) -> serde_json::Value {
    let video_bytes = std::fs::metadata(&movie.path).map(|m| m.len()).unwrap_or(0);
    json!({
        "sidecar_format": SIDECAR_FORMAT,
        "recipe": encode_recipe(),
        "clips_provenance": format!(
            "{:016x}",
            xxhash_rust::xxh3::xxh3_64(&serde_json::to_vec(provenance).unwrap_or_default())
        ),
        "video": {
            "filename": movie.path.file_name().and_then(|f| f.to_str()),
            "bytes": video_bytes,
            "audio_stream": audio_stream,
        },
    })
}

pub async fn export_clips(
    out: PathBuf,
    dest: PathBuf,
    jobs: usize,
    limit: usize,
    imdb: Option<String>,
    langs: Option<Vec<String>>,
) -> Result<()> {
    // Cache misses must fail the clip's alignment block, never call Modal.
    phoneme_verify::set_cache_only(true);
    let plan = read_plan(&out)?;
    let mut queue: Vec<Movie> = plan
        .into_iter()
        .filter(|m| imdb.as_deref().is_none_or(|id| m.imdb_id == id))
        .filter(|m| {
            langs.as_ref().is_none_or(|l| {
                course_dir(&m.original_language).is_some_and(|c| l.iter().any(|x| x == c))
            })
        })
        .filter(|m| clips_path(&out.join(&m.imdb_id)).exists())
        .collect();
    if limit > 0 {
        queue.truncate(limit);
    }
    println!("{} films with clips to export", queue.len());

    let store = osmo::Store::open("./.cache");
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let mut written = 0usize;
    let mut skipped = 0usize;
    let mut failed = false;
    let mut valid: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for movie in &queue {
        let title = truncate(&movie.title, 34);
        match export_film(&http, &store, movie, &out, &dest, jobs).await {
            Ok(f) => {
                written += f.written;
                skipped += f.skipped;
                println!(
                    "{title} ✓ {} exported, {} already current",
                    f.written, f.skipped
                );
                valid.entry(f.code).or_default().extend(f.ids);
            }
            Err(e) => {
                failed = true;
                println!("{title} ✗ {e:#}");
            }
        }
    }
    println!("\n{written} clips exported, {skipped} already current");

    // Orphan sweep: a clip dir whose id no longer exists (sentence re-keyed,
    // gate change) must not linger looking servable. Only on unfiltered,
    // fully-successful runs — a partial run cannot know the full id set.
    if imdb.is_none() && limit == 0 && !failed {
        for (code, ids) in &valid {
            let lang_dir = dest.join(code);
            let mut swept = 0usize;
            for entry in std::fs::read_dir(&lang_dir).into_iter().flatten().flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() && !ids.contains(&name) {
                    std::fs::remove_dir_all(&path)?;
                    swept += 1;
                }
            }
            if swept > 0 {
                println!("swept {swept} orphaned clip dirs from {code}");
            }
        }
    }

    // The index is rebuilt from the sidecars on every run, so resumed and
    // partial runs still leave it whole.
    for lang in queue
        .iter()
        .filter_map(|m| course_dir(&m.original_language))
        .collect::<std::collections::BTreeSet<_>>()
    {
        let n = write_index(&dest.join(lang))?;
        println!("index: {lang} {n} clips");
    }
    Ok(())
}

async fn export_film(
    http: &reqwest::Client,
    store: &osmo::Store,
    movie: &Movie,
    out: &Path,
    dest: &Path,
    jobs: usize,
) -> Result<FilmExport> {
    let code = course_dir(&movie.original_language).context("unmapped language")?;
    let language = Language::from_code(code).context("unmapped course code")?;
    let dir = out.join(&movie.imdb_id);
    if !movie.path.exists() {
        bail!("video missing: {}", movie.path.display());
    }

    let (provenance, clips) = read_clips_with_provenance(&clips_path(&dir))?;
    let srt = std::fs::read_to_string(dir.join("subtitle.srt"))?;
    let segmenter = SubtitleSegmenter::for_language(language)?;
    let sentences = subtitle_sentences(&srt, language, &segmenter);
    let cues: Vec<Cue> = parse_cues(&srt)
        .into_iter()
        .filter_map(|c| {
            let text = repair_latin_homoglyphs(&cleanup_subtitle_text(&c.text));
            (!text.is_empty()).then_some(Cue { text, ..c })
        })
        .collect();
    let transcript = load_transcript(&dir.join("transcript.jsonl"))?;
    let audio_stream = audio_stream_index(&dir.join("audio.json"), &movie.path)?;
    let video = probe_video(&movie.path)?;
    let audio_opus = dir.join("audio.opus");

    let empty = std::collections::HashMap::new();
    let ctx = VerifyContext::new(http, store.clone(), &empty, language)?;
    let stamp = export_stamp(&provenance, movie, audio_stream);

    let lang_dir = dest.join(code);
    let passing: Vec<&Clip> = clips.iter().filter(|c| c.passed).collect();
    let done = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);
    let total = passing.len();

    use futures::StreamExt;
    let results: Vec<Result<()>> = futures::stream::iter(passing.iter().map(|clip| {
        let (ctx, movie, provenance, clips, sentences, cues, transcript) = (
            &ctx,
            movie,
            &provenance,
            &clips,
            &sentences,
            &cues,
            &transcript,
        );
        let stamp = &stamp;
        let (lang_dir, audio_opus, video) = (&lang_dir, &audio_opus, &video);
        let (done, skipped) = (&done, &skipped);
        async move {
            let id = clip_id(&movie.imdb_id, clip, sentences, clips);
            let clip_dir = lang_dir.join(&id);
            let current = std::fs::read(clip_dir.join("meta.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .is_some_and(|m| {
                    m["export"] == *stamp
                        && clip_dir.join("hi.mp4").exists()
                        && clip_dir.join("lo.mp4").exists()
                });
            if current {
                skipped.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
            let _ = std::fs::remove_dir_all(&clip_dir);
            let course_sentence = sentences
                .iter()
                .any(|k| k.course_worthy && k.sentence == clip.sentence);
            let r = export_one(
                ctx,
                movie,
                provenance,
                clip,
                &id,
                course_sentence,
                stamp,
                cues,
                transcript,
                audio_opus,
                video,
                audio_stream,
                &clip_dir,
            )
            .await;
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            match &r {
                Ok(()) => println!("  [{n}/{total}] {id} ✓"),
                Err(e) => println!("  [{n}/{total}] {id} ✗ {e:#}"),
            }
            r
        }
    }))
    .buffer_unordered(jobs.max(1))
    .collect()
    .await;

    let failed = results.iter().filter(|r| r.is_err()).count();
    if failed > 0 {
        bail!("{failed} of {total} clips failed");
    }
    let ids = passing
        .iter()
        .map(|clip| clip_id(&movie.imdb_id, clip, &sentences, &clips))
        .collect();
    Ok(FilmExport {
        written: done.load(Ordering::Relaxed),
        skipped: skipped.load(Ordering::Relaxed),
        code: code.to_string(),
        ids,
    })
}

/// What one film's export produced, for totals and the orphan sweep.
struct FilmExport {
    written: usize,
    skipped: usize,
    code: String,
    ids: Vec<String>,
}

/// `imdb - sha256(NFC sentence)[..8] - occurrence index`, the occurrence
/// counted over the segmented subtitle sentences in cue order (all of them,
/// aligned or not), so the id is fixed by the subtitle file alone.
fn clip_id(imdb: &str, clip: &Clip, sentences: &[KeyedSentence], all: &[Clip]) -> String {
    let normalized: String = clip.sentence.nfc().collect();
    let digest = Sha256::digest(normalized.as_bytes());
    let hash: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();

    // Occurrences of this sentence, in subtitle order, with passage spans.
    let occ: Vec<(i64, i64)> = sentences
        .iter()
        .filter(|k| k.sentence == clip.sentence)
        .map(|k| (i64::from(k.start_ms), i64::from(k.end_ms)))
        .collect();
    let index = match occ.len() {
        0 | 1 => 0,
        _ => {
            // Pick the occurrence whose passage span sits closest to this
            // clip's audio span. Cue stamps are display times, so exact
            // containment can miss; nearest midpoint is unambiguous when the
            // same line recurs minutes apart.
            let mid = (clip.start_ms + clip.end_ms) / 2;
            occ.iter()
                .enumerate()
                .min_by_key(|(_, (a, b))| ((a + b) / 2 - mid).abs())
                .map(|(i, _)| i)
                .unwrap_or(0)
        }
    };
    // Sanity: two clips of the same sentence must not land on one id. If the
    // nearest-midpoint tie-break ever collides (identical spans), fall back
    // to rank among same-sentence clips by start time.
    let twins: Vec<&Clip> = all.iter().filter(|c| c.sentence == clip.sentence).collect();
    let index = if twins.len() > occ.len() {
        twins
            .iter()
            .position(|c| c.start_ms == clip.start_ms)
            .unwrap_or(index)
    } else {
        index
    };
    format!("{imdb}-{hash}-{index}")
}

#[allow(clippy::too_many_arguments)]
async fn export_one(
    ctx: &VerifyContext<'_>,
    movie: &Movie,
    provenance: &Provenance,
    clip: &Clip,
    id: &str,
    course_sentence: bool,
    stamp: &serde_json::Value,
    cues: &[Cue],
    transcript: &[Spoken],
    audio_opus: &Path,
    video: &VideoProbe,
    audio_stream: u32,
    clip_dir: &Path,
) -> Result<()> {
    // The scored cut (what the gates heard) and the generous cut around it.
    let scored_start = (clip.start_ms - clip.pad_before_ms).max(0);
    let scored_end = clip.end_ms + clip.pad_after_ms;
    let (cut_start, cut_end, ctx_before, ctx_after) =
        context_bounds(clip, scored_start, scored_end, cues);
    let critical = (scored_start - cut_start, scored_end - cut_start);

    // Forced alignment from the cached frame matrix — same wav bytes the
    // gates scored, so this is a cache hit unless the cut code drifted.
    // Failure loses the alignment block, never the clip.
    let alignment = align_phonemes(ctx, audio_opus, clip, scored_start - cut_start).await;

    // Loudness of the critical span through the same stereo downmix the
    // encode uses; gain capped by the true-peak ceiling.
    let (measured_i, measured_tp) = tokio::task::spawn_blocking({
        let path = movie.path.clone();
        move || measure_loudness(&path, audio_stream, scored_start, scored_end - scored_start)
    })
    .await??;
    let gain_db = (TARGET_I - measured_i).min(TP_CEIL - measured_tp);

    std::fs::create_dir_all(clip_dir)?;
    let encode = tokio::task::spawn_blocking({
        let (path, clip_dir) = (movie.path.clone(), clip_dir.to_path_buf());
        let video = video.clone();
        let crit_s = critical.0 as f64 / 1000.0;
        move || {
            encode_renditions(
                &path,
                audio_stream,
                &video,
                cut_start,
                cut_end,
                gain_db,
                crit_s,
                &clip_dir,
            )
        }
    })
    .await?;
    if let Err(e) = encode {
        // A half-written directory must not read as done on resume.
        let _ = std::fs::remove_dir_all(clip_dir);
        return Err(e);
    }

    let rel = |ms: i64| ms - cut_start;
    let sidecar = json!({
        "format": SIDECAR_FORMAT,
        "export": stamp,
        "id": id,
        "language": provenance.language,
        "film": {
            "imdb_id": movie.imdb_id,
            "title": movie.title,
            "year": movie.year,
            "subtitle_digest": provenance.subtitle_digest,
            "transcript_digest": provenance.transcript_digest,
        },
        "source": {
            "sentence_start_ms": clip.start_ms,
            "sentence_end_ms": clip.end_ms,
            "cut_start_ms": cut_start,
            "cut_end_ms": cut_end,
            "pad_before_ms": clip.pad_before_ms,
            "pad_after_ms": clip.pad_after_ms,
            "repaired_before_ms": clip.repaired_before_ms,
            "repaired_after_ms": clip.repaired_after_ms,
        },
        "critical": { "start_ms": critical.0, "end_ms": critical.1 },
        "sentence": {
            "text": clip.sentence,
            "course_sentence": course_sentence,
            "speaker": clip.speaker,
            "words": clip.words.iter().map(|w| json!({
                "text": w.text, "at_ms": rel(w.at_ms), "until_ms": rel(w.until_ms),
            })).collect::<Vec<_>>(),
        },
        "subtitles": cues.iter()
            .filter(|c| c.end_ms > cut_start && c.start_ms < cut_end)
            .map(|c| {
                let role = if c.end_ms <= clip.start_ms { "context-before" }
                    else if c.start_ms >= clip.end_ms { "context-after" }
                    else { "sentence" };
                json!({
                    "text": c.text,
                    "at_ms": rel(c.start_ms), "until_ms": rel(c.end_ms),
                    "role": role,
                })
            }).collect::<Vec<_>>(),
        "context_verified": false,
        "context_lines": { "before": ctx_before, "after": ctx_after },
        "transcript": {
            "words": transcript.iter()
                .filter(|w| w.until_ms > cut_start && w.at_ms < cut_end)
                .map(|w| json!({
                    "text": w.text,
                    "at_ms": rel(w.at_ms), "until_ms": rel(w.until_ms),
                    "kind": if w.kind == Kind::AudioEvent { "audio_event" } else { "word" },
                    "speaker": w.speaker, "logprob": w.logprob,
                })).collect::<Vec<_>>(),
        },
        "phonemes": {
            "target_ipa": clip.target_ipa,
            "heard_ipa": clip.heard_ipa,
            "oov": clip.oov,
            "alignment": alignment.unwrap_or(serde_json::Value::Null),
        },
        "verification": {
            "passed": clip.passed,
            "reject": clip.reject,
            "transcript_wer": clip.transcript_wer,
            "ratio": clip.ratio,
            "logp_target_per_phoneme": clip.logp_target_per_phoneme,
            "edge_logp_start": clip.edge_logp_start,
            "edge_logp_end": clip.edge_logp_end,
            "lead_speech": clip.lead_speech,
            "tail_speech": clip.tail_speech,
            "lead_rms": clip.lead_rms,
            "voiced": clip.voiced,
            "audio_event_overlap": clip.audio_event_overlap,
            "clear_before_ms": clip.clear_before_ms,
            "clear_after_ms": clip.clear_after_ms,
            "provenance": {
                "format": provenance.format,
                "model": provenance.model,
                "min_ratio": provenance.min_ratio,
                "min_clear_ms": provenance.min_clear_ms,
                "min_edge_logp": provenance.min_edge_logp,
                "max_pad_speech": provenance.max_pad_speech,
                "max_lead_rms": provenance.max_lead_rms,
                "min_voiced": provenance.min_voiced,
            },
        },
        "media": {
            "duration_ms": cut_end - cut_start,
            "loudnorm": {
                "measured_i": measured_i,
                "measured_tp": measured_tp,
                "gain_db": gain_db,
                "measured_over": "critical",
            },
            "keyframe_at_critical": true,
            "renditions": {
                "hi": rendition_info(&clip_dir.join("hi.mp4"), video.height.min(MAX_HEIGHT)),
                "lo": rendition_info(&clip_dir.join("lo.mp4"), video.height.min(LO_HEIGHT)),
            },
        },
    });
    // Write-then-rename so `meta.json exists` really means the clip is whole.
    let tmp = clip_dir.join("meta.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&sidecar)?)?;
    std::fs::rename(&tmp, clip_dir.join("meta.json"))?;
    Ok(())
}

/// The full serve pipeline, [`crate::clips`]-style resumable at every stage:
/// re-map clips (skips films whose provenance is current), export videos
/// (skips clip dirs with a finished sidecar), upload to R2 (skips `.uploaded`
/// markers). Safe to re-run after any interruption.
pub async fn publish(
    out: PathBuf,
    dest: PathBuf,
    jobs: usize,
    langs: Option<Vec<String>>,
    bucket: String,
) -> Result<()> {
    println!("=== stage 1: clips re-map ===");
    let gate = crate::clips::Gate::default();
    crate::clips::clips_all(out.clone(), 4, 0, None, langs.clone(), gate).await?;

    println!("=== stage 2: video export ===");
    export_clips(out, dest.clone(), jobs, 0, None, langs.clone()).await?;

    println!("=== stage 3: upload to {bucket} ===");
    let codes: Vec<String> = match &langs {
        Some(l) => l.clone(),
        None => std::fs::read_dir(&dest)?
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect(),
    };
    for code in codes {
        upload_lang(&dest.join(&code), &code, &bucket)?;
    }
    Ok(())
}

/// Upload one language's exported clips via wrangler. A `.uploaded` marker is
/// written per clip dir once all three objects land; marked dirs are skipped,
/// so re-runs only pay for what's new. Objects are immutable by id and get a
/// forever cache; the index gets a short one.
fn upload_lang(lang_dir: &Path, code: &str, bucket: &str) -> Result<()> {
    const IMMUTABLE: &str = "public, max-age=31536000, immutable";
    let put = |file: &Path, key: &str, content_type: &str, cache: &str| -> Result<()> {
        let status = Command::new("wrangler")
            .args(["r2", "object", "put", &format!("{bucket}/{key}")])
            .arg("--file")
            .arg(file)
            .args(["--content-type", content_type, "--cache-control", cache])
            .arg("--remote")
            .stdout(std::process::Stdio::null())
            .status()
            .context("wrangler failed to start")?;
        if !status.success() {
            bail!("upload failed for {key}");
        }
        Ok(())
    };
    let mut uploaded = 0usize;
    let mut skipped = 0usize;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(lang_dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let id = dir.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        if !dir.join("meta.json").exists() {
            continue; // half-written export
        }
        // The marker records what was uploaded, not that something was:
        // skip only when every file still hashes to what went up. An edited
        // sidecar or re-rendered mp4 re-uploads; a stale marker never wins.
        let hashes = json!({
            "hi": file_hash(&dir.join("hi.mp4"))?,
            "lo": file_hash(&dir.join("lo.mp4"))?,
            "meta": file_hash(&dir.join("meta.json"))?,
        });
        let marked = std::fs::read(dir.join(".uploaded"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        if marked.as_ref() == Some(&hashes) {
            skipped += 1;
            continue;
        }
        put(
            &dir.join("hi.mp4"),
            &format!("{code}/{id}/hi.mp4"),
            "video/mp4",
            IMMUTABLE,
        )?;
        put(
            &dir.join("lo.mp4"),
            &format!("{code}/{id}/lo.mp4"),
            "video/mp4",
            IMMUTABLE,
        )?;
        put(
            &dir.join("meta.json"),
            &format!("{code}/{id}/meta.json"),
            "application/json",
            IMMUTABLE,
        )?;
        std::fs::write(dir.join(".uploaded"), serde_json::to_vec(&hashes)?)?;
        uploaded += 1;
        if uploaded.is_multiple_of(25) {
            println!("  {uploaded} uploaded (last: {id})");
        }
    }
    let index = lang_dir.join("index.jsonl");
    if index.exists() {
        put(
            &index,
            &format!("{code}/index.jsonl"),
            "application/x-ndjson",
            "public, max-age=60",
        )?;
    }
    println!("{code}: {uploaded} clip dirs uploaded, {skipped} already up");
    Ok(())
}

/// Extend the scored cut to neighboring subtitle lines within [`CTX_GAP_MS`],
/// then trim (furthest line first) back under [`CTX_CAP_MS`]. Returns the cut
/// bounds (film-absolute) and how many lines survived on each side.
fn context_bounds(
    clip: &Clip,
    scored_start: i64,
    scored_end: i64,
    cues: &[Cue],
) -> (i64, i64, usize, usize) {
    let mut before: Vec<&Cue> = Vec::new();
    let mut edge = clip.start_ms;
    for cue in cues.iter().rev().filter(|c| c.end_ms <= clip.start_ms) {
        if edge - cue.end_ms > CTX_GAP_MS {
            break;
        }
        edge = cue.start_ms;
        before.push(cue);
    }
    let mut after: Vec<&Cue> = Vec::new();
    let mut edge = clip.end_ms;
    for cue in cues.iter().filter(|c| c.start_ms >= clip.end_ms) {
        if cue.start_ms - edge > CTX_GAP_MS {
            break;
        }
        edge = cue.end_ms;
        after.push(cue);
    }
    let bounds = |before: &[&Cue], after: &[&Cue]| {
        let s = before
            .last()
            .map_or(scored_start, |c| {
                (c.start_ms - CTX_PAD_MS).min(scored_start)
            })
            .max(0);
        let e = after
            .last()
            .map_or(scored_end, |c| (c.end_ms + CTX_PAD_MS).max(scored_end));
        (s, e)
    };
    let (mut s, mut e) = bounds(&before, &after);
    while e - s > CTX_CAP_MS && (!before.is_empty() || !after.is_empty()) {
        // Drop whichever outermost line sits furthest from the sentence.
        let d_before = before.last().map(|c| clip.start_ms - c.start_ms);
        let d_after = after.last().map(|c| c.end_ms - clip.end_ms);
        if d_before >= d_after {
            before.pop();
        } else {
            after.pop();
        }
        (s, e) = bounds(&before, &after);
    }
    (s, e, before.len(), after.len())
}

/// Per-phoneme spans in clip-relative ms, from the cached frame matrix.
async fn align_phonemes(
    ctx: &VerifyContext<'_>,
    audio_opus: &Path,
    clip: &Clip,
    scored_offset_ms: i64,
) -> Option<serde_json::Value> {
    let wav = slice_wav_padded(
        audio_opus,
        clip.start_ms,
        clip.end_ms,
        clip.pad_before_ms,
        clip.pad_after_ms,
    )
    .ok()?;
    let frames = phoneme_verify::frame_matrix(ctx, &wav).await.ok()?;
    let present: Vec<&String> = clip
        .target_ipa
        .iter()
        .filter(|t| frames.id(t).is_some())
        .collect();
    let ids: Vec<usize> = present.iter().filter_map(|t| frames.id(t)).collect();
    let spans = frames.force_align(&ids)?;
    // Frames cover the wav after symmetric zero-padding to the model's
    // 0.6s minimum; undo that padding to place frames in wav time.
    let wav_ms = clip.end_ms + clip.pad_after_ms - (clip.start_ms - clip.pad_before_ms).max(0);
    let padded_ms = wav_ms.max(600);
    let lead_pad_ms = (padded_ms - wav_ms) / 2;
    let ms_per_frame = padded_ms as f64 / frames.frames as f64;
    Some(
        spans
            .iter()
            .zip(&present)
            .map(|(s, ph)| {
                let at = (s.start_frame as f64 * ms_per_frame) as i64 - lead_pad_ms;
                let until = ((s.end_frame + 1) as f64 * ms_per_frame) as i64 - lead_pad_ms;
                json!({
                    "ph": ph,
                    "at_ms": at + scored_offset_ms,
                    "until_ms": until + scored_offset_ms,
                    "logp": s.logp_mean,
                })
            })
            .collect(),
    )
}

#[derive(Debug, Clone)]
struct VideoProbe {
    height: i64,
    /// PQ / HLG sources are tonemapped to SDR bt709 for h264 playback.
    hdr: bool,
}

fn probe_video(path: &Path) -> Result<VideoProbe> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries"])
        .args(["stream=height,color_transfer", "-of", "json"])
        .arg(path)
        .output()
        .context("ffprobe failed to start")?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).context("ffprobe output")?;
    let stream = v["streams"].get(0).context("no video stream")?;
    let height = stream["height"].as_i64().context("no height")?;
    let transfer = stream["color_transfer"].as_str().unwrap_or("");
    Ok(VideoProbe {
        height,
        hdr: matches!(transfer, "smpte2084" | "arib-std-b67"),
    })
}

/// The audio-relative stream index (`-map 0:a:N`) recorded at extraction,
/// verified against the file on disk — a remux can reorder audio tracks
/// under an unchanged filename, and the wrong language track must fail loud.
fn audio_stream_index(audio_json: &Path, video: &Path) -> Result<u32> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(audio_json)?)?;
    let index = v["stream"]["stream_index"]
        .as_u64()
        .context("audio.json has no stream_index")?;
    let identity = crate::sync::audio_stream_identity(video, index as usize)?;
    let (codec, channels) = (
        v["stream"]["codec"].as_str().unwrap_or_default(),
        v["stream"]["channels"].as_u64().unwrap_or_default() as u32,
    );
    if identity.codec != codec || identity.channels != channels {
        bail!(
            "audio stream a:{index} is now {}/{}ch, extraction saw {codec}/{channels}ch — \
             remux changed under {}",
            identity.codec,
            identity.channels,
            video.display()
        );
    }
    Ok(index as u32)
}

/// EBU R128 integrated loudness + true peak of one span, through the same
/// stereo downmix the encode applies.
fn measure_loudness(
    path: &Path,
    audio_stream: u32,
    start_ms: i64,
    dur_ms: i64,
) -> Result<(f64, f64)> {
    let out = Command::new("ffmpeg")
        .args(["-nostats", "-hide_banner", "-ss"])
        .arg(format!("{:.3}", start_ms as f64 / 1000.0))
        .args(["-t", &format!("{:.3}", dur_ms as f64 / 1000.0), "-i"])
        .arg(path)
        .args(["-map", &format!("0:a:{audio_stream}")])
        .args(["-af", "aformat=channel_layouts=stereo,ebur128=peak=true"])
        .args(["-f", "null", "-"])
        .output()
        .context("ffmpeg (loudness) failed to start")?;
    let text = String::from_utf8_lossy(&out.stderr);
    let summary = text
        .rsplit("Summary:")
        .next()
        .context("no ebur128 summary")?;
    let grab = |label: &str| -> Option<f64> {
        summary
            .lines()
            .find(|l| l.trim_start().starts_with(label))?
            .split_whitespace()
            .find_map(|t| t.parse::<f64>().ok())
    };
    let i = grab("I:").context("no integrated loudness in summary")?;
    // "Peak:" appears under both "Sample peak" and "True peak"; the last one
    // is the true peak.
    let tp = summary
        .lines()
        .filter(|l| l.trim_start().starts_with("Peak:"))
        .filter_map(|l| l.split_whitespace().find_map(|t| t.parse::<f64>().ok()))
        .next_back()
        .context("no true peak in summary")?;
    Ok((i, tp))
}

/// One decode of the source segment, two encodes: `hi.mp4` (≤1440p, quality)
/// and `lo.mp4` (≤480p, fast first paint). Both get a keyframe at the
/// critical start and faststart moov.
#[allow(clippy::too_many_arguments)]
fn encode_renditions(
    path: &Path,
    audio_stream: u32,
    video: &VideoProbe,
    cut_start: i64,
    cut_end: i64,
    gain_db: f64,
    crit_s: f64,
    clip_dir: &Path,
) -> Result<()> {
    let tonemap = if video.hdr {
        "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=hable:desat=0,\
         zscale=t=bt709:m=bt709:r=tv,"
    } else {
        ""
    };
    let filter = format!(
        "[0:v:0]{tonemap}scale=-2:'min({MAX_HEIGHT},ih)':flags=lanczos,format=yuv420p,\
         split=2[vh][v0];[v0]scale=-2:'min({LO_HEIGHT},ih)'[vl];\
         [0:a:{audio_stream}]aformat=channel_layouts=stereo,volume={gain_db:.2}dB,\
         aresample=48000,asplit=2[ah][al]"
    );
    let key = format!("{crit_s:.3}");
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-ss"])
        .arg(format!("{:.3}", cut_start as f64 / 1000.0))
        .args([
            "-t",
            &format!("{:.3}", (cut_end - cut_start) as f64 / 1000.0),
        ])
        .arg("-i")
        .arg(path)
        .args(["-filter_complex", &filter])
        .args(["-map", "[vh]", "-map", "[ah]"])
        .args(["-c:v", "libx264", "-crf", "19", "-preset", "medium"])
        .args(["-c:a", "aac", "-b:a", "160k"])
        .args(["-force_key_frames", &key, "-movflags", "+faststart"])
        .arg(clip_dir.join("hi.mp4"))
        .args(["-map", "[vl]", "-map", "[al]"])
        .args(["-c:v", "libx264", "-crf", "27", "-preset", "veryfast"])
        .args(["-c:a", "aac", "-b:a", "96k"])
        .args(["-force_key_frames", &key, "-movflags", "+faststart"])
        .arg(clip_dir.join("lo.mp4"))
        .status()
        .context("ffmpeg (encode) failed to start")?;
    if !status.success() {
        bail!("ffmpeg encode failed for {}", clip_dir.display());
    }
    Ok(())
}

fn file_hash(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("hashing {}", path.display()))?;
    Ok(format!("{:016x}", xxhash_rust::xxh3::xxh3_64(&bytes)))
}

fn rendition_info(file: &Path, height: i64) -> serde_json::Value {
    json!({
        "file": file.file_name().and_then(|s| s.to_str()),
        "height": height,
        "bytes": std::fs::metadata(file).map(|m| m.len()).unwrap_or(0),
    })
}

fn read_clips_with_provenance(path: &Path) -> Result<(Provenance, Vec<Clip>)> {
    let text = std::fs::read_to_string(path)?;
    let first = text.lines().next().context("empty clips.jsonl")?;
    let provenance: Provenance = serde_json::from_str(first).context("no provenance line")?;
    Ok((provenance, read_clips(path)?))
}

/// One line per exported clip, rebuilt from the sidecars.
fn write_index(lang_dir: &Path) -> Result<usize> {
    let mut rows: Vec<(String, serde_json::Value)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(lang_dir) {
        for entry in entries.flatten() {
            let meta = entry.path().join("meta.json");
            let Ok(bytes) = std::fs::read(&meta) else {
                continue;
            };
            let m: serde_json::Value = serde_json::from_slice(&bytes)?;
            rows.push((
                m["id"].as_str().unwrap_or_default().to_string(),
                json!({
                    "id": m["id"],
                    "imdb_id": m["film"]["imdb_id"],
                    "title": m["film"]["title"],
                    "sentence": m["sentence"]["text"],
                    "course_sentence": m["sentence"]["course_sentence"],
                    "duration_ms": m["media"]["duration_ms"],
                    "critical": m["critical"],
                    "hi_bytes": m["media"]["renditions"]["hi"]["bytes"],
                    "lo_bytes": m["media"]["renditions"]["lo"]["bytes"],
                }),
            ));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let n = rows.len();
    let body: String = rows.into_iter().map(|(_, v)| format!("{v}\n")).collect();
    std::fs::write(lang_dir.join("index.jsonl"), body)?;
    Ok(n)
}
