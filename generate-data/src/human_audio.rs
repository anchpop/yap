//! Loading and verification of human-recorded audio for a language.
//!
//! Reads per-contributor recordings declared under `<source_data>/audio/`,
//! runs each clip through the phonemic verifier (see [`crate::audio_verification`])
//! so badly-mismatched takes never reach the language pack, and encodes the
//! survivors to OGG/Opus for shipping.

use anyhow::Context;
use futures::{StreamExt, TryStreamExt};
use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Load human-recorded audio for a language from `<source_data>/audio/<credit>/`.
///
/// Each contributor lives in their own subdirectory whose name becomes the credit.
/// The subdirectory must contain a `manifest.jsonl` mapping target-language text
/// to a relative .wav filename: `{"text": "tu", "file": "01a_tu.wav"}`.
///
/// Compensation (paid vs. volunteer) is declared in `<source_data>/audio/voice-actors.json`,
/// which must contain an entry for every contributor subdirectory. We require this
/// to be explicit so we never accidentally claim a volunteer contribution is paid.
///
/// Each .wav is encoded to OGG/Opus via ffmpeg. The browser already supports this
/// format (see `is_valid_audio_data` in yap-frontend-rs/src/audio.rs).
pub async fn load_human_audio(
    source_data_path: &Path,
    word_to_pronunciation: &[(String, language_utils::Pronunciations)],
    failures_log: &Path,
    all_results_log: &Path,
    target_language: language_utils::Language,
    http: &reqwest::Client,
) -> anyhow::Result<FxHashMap<language_utils::VoiceActor, FxHashMap<String, language_utils::Audio>>>
{
    let audio_root = source_data_path.join("audio");
    let mut out: FxHashMap<language_utils::VoiceActor, FxHashMap<String, language_utils::Audio>> =
        FxHashMap::default();

    // Wipe stale verification logs up front so each file reflects only the
    // most recent run — they're re-created at the end only if this run
    // produces matching results.
    for log_path in [failures_log, all_results_log] {
        if log_path.exists() {
            std::fs::remove_file(log_path)
                .with_context(|| format!("Failed to remove stale {}", log_path.display()))?;
        }
    }

    if !audio_root.exists() {
        return Ok(out);
    }

    #[derive(serde::Deserialize)]
    struct ManifestEntry {
        text: String,
        file: String,
        /// Optional speaker-realized IPA, whitespace-separated. When
        /// present, the verifier scores model output against this exact
        /// sequence instead of the language-wide wikipron+espeak ground
        /// truth — use to encode connected-speech realizations that
        /// differ from citation form (e.g. French `de` produced as
        /// /dø/ by speakers who merge unstressed schwa with ø).
        #[serde(default)]
        phonemic_transcription: Option<String>,
    }

    /// Quality tier for a voice actor. Drives whether their clips end up
    /// in the shipped language pack — only `High` ships; `Medium` and `Low`
    /// are kept in the repo for diagnostics / verifier benchmarking but
    /// never reach users. An entry that omits `quality` defaults to
    /// `Medium`, so clips ship only when an actor is *explicitly* marked
    /// `High` — an unannotated actor never reaches users by accident.
    #[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
    #[serde(rename_all = "lowercase")]
    enum VoiceActorQuality {
        High,
        #[default]
        Medium,
        Low,
    }

    #[derive(serde::Deserialize)]
    struct VoiceActorEntry {
        compensation: language_utils::Compensation,
        #[serde(default)]
        quality: VoiceActorQuality,
    }

    let mut contributors: Vec<PathBuf> = std::fs::read_dir(&audio_root)
        .with_context(|| format!("Failed to read audio dir {}", audio_root.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    contributors.sort();

    let voice_actors_path = audio_root.join("voice-actors.json");
    let voice_actors: FxHashMap<String, VoiceActorEntry> = if contributors.is_empty() {
        FxHashMap::default()
    } else {
        let text = std::fs::read_to_string(&voice_actors_path).with_context(|| {
            format!(
                "Missing {} — required to declare compensation for each contributor",
                voice_actors_path.display()
            )
        })?;
        serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse {}", voice_actors_path.display()))?
    };

    // Cache root for wav2vec2 predictions. Lives at repo-root .cache, same
    // location used by other generate-data caches.
    let cache_root = PathBuf::from(".cache");
    let wp_lookup: std::collections::HashMap<String, language_utils::Pronunciations> =
        word_to_pronunciation
            .iter()
            .map(|(w, p)| (w.to_lowercase(), p.clone()))
            .collect();
    let verify_ctx = crate::audio_verification::VerifyContext::new(
        http,
        &cache_root,
        &wp_lookup,
        target_language,
    )?;

    let mut failures: Vec<crate::audio_verification::ClipVerification> = Vec::new();
    let mut all_results: Vec<crate::audio_verification::ClipVerification> = Vec::new();

    for contributor_dir in contributors {
        let name = contributor_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid contributor dir: {contributor_dir:?}"))?
            .to_string();
        let entry = voice_actors.get(&name).ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no entry for contributor {:?} — add one with compensation: \"paid\" or \"volunteer\"",
                voice_actors_path.display(),
                name
            )
        })?;
        let compensation = entry.compensation;
        let quality = entry.quality;
        let actor = language_utils::VoiceActor { name, compensation };
        let manifest = contributor_dir.join("manifest.jsonl");
        if !manifest.exists() {
            log::warn!("audio dir {} has no manifest.jsonl; skipping", actor.name);
            continue;
        }
        let manifest_text = std::fs::read_to_string(&manifest)
            .with_context(|| format!("Failed to read {}", manifest.display()))?;

        let mut entries: Vec<ManifestEntry> = Vec::new();
        for (line_no, line) in manifest_text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            entries.push(serde_json::from_str(line).with_context(|| {
                format!("Failed to parse {}:{}", manifest.display(), line_no + 1)
            })?);
        }

        let clips = out.entry(actor.clone()).or_default();

        // Verify clips in parallel — Modal calls are I/O-bound and slow, and
        // running serially over a couple hundred clips is the dominant time
        // cost on first runs. Buffered(8) is a polite concurrency level.
        let v_ctx = &verify_ctx;
        let dir_ref = &contributor_dir;
        let actor_name_ref = actor.name.as_str();
        let verifications: Vec<_> = futures::stream::iter(&entries)
            .map(|entry| async move {
                let wav_path = dir_ref.join(&entry.file);
                let expected = crate::audio_verification::expected_phoneme_variants(
                    v_ctx,
                    &entry.text,
                    entry.phonemic_transcription.as_deref(),
                );
                let v = crate::audio_verification::verify_clip(
                    v_ctx,
                    actor_name_ref,
                    &entry.text,
                    &wav_path,
                    expected,
                )
                .await?;
                anyhow::Ok((entry, wav_path, v))
            })
            .buffered(8)
            .try_collect()
            .await?;

        let mut pass_count = 0usize;
        let mut fail_count = 0usize;
        let mut excluded_for_quality = 0usize;
        // Lower-quality actors still get fully verified (we want the
        // diagnostic data in audio_verification_all.jsonl), but their
        // passing clips don't reach the shipped language pack.
        let ship_clips = matches!(quality, VoiceActorQuality::High);
        for (entry, wav_path, verification) in verifications {
            all_results.push(verification.clone());
            if let Some(reason) = &verification.failure_reason {
                log::info!(
                    "audio verification rejected {:?} from {}: {reason}",
                    entry.text,
                    actor.name
                );
                fail_count += 1;
                failures.push(verification);
                continue;
            }
            pass_count += 1;
            if !ship_clips {
                excluded_for_quality += 1;
                continue;
            }
            let bytes = encode_wav_to_opus(&wav_path)
                .with_context(|| format!("Failed to encode {} to opus", wav_path.display()))?;
            // TODO: keep every take rather than last-take-wins. Several
            // manifest texts are recorded more than once (alternate takes we
            // commissioned), but the `(actor, text) -> Audio` map discards all
            // but the last. The plan: make this `(actor, text) -> Vec<Audio>`
            // and have the frontend `human_audio::lookup` registry rotate
            // through an actor's takes the same way it already rotates across
            // actors, so learners hear natural variation instead of one clip.
            if clips
                .insert(entry.text.clone(), language_utils::Audio { bytes })
                .is_some()
            {
                log::warn!(
                    "duplicate clip for {:?} from {}: keeping the later one",
                    entry.text,
                    actor.name
                );
            }
        }
        let quality_note = if !ship_clips {
            format!(", {excluded_for_quality} excluded from pack (quality={quality:?})")
        } else {
            String::new()
        };
        println!(
            "Loaded human audio from {} ({pass_count} passed, {fail_count} rejected{quality_note})",
            contributor_dir.display()
        );
        if clips.is_empty() {
            out.remove(&actor);
        }
    }

    // Run Google TTS on every unique text in the manifests and verify the
    // synthetic audio. Diagnostic only — we don't add TTS audio to the
    // dataset. Comprehensive coverage (every text, not just ones where
    // humans failed) lets us establish a verifier baseline: model failures
    // on clean studio audio are model bias, not recording quality.
    if let (Ok(api_key), Some(voice)) = (
        std::env::var("GOOGLE_CLOUD_API_KEY"),
        crate::audio_verification::default_voice_for(target_language),
    ) {
        let mut unique_texts: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for v in &all_results {
            unique_texts.insert(v.text.clone());
        }
        let texts: Vec<String> = unique_texts.into_iter().collect();

        if !texts.is_empty() {
            println!(
                "Running Google TTS verification on all {} unique text(s)...",
                texts.len()
            );
            let api_key_ref = api_key.as_str();
            let v_ctx = &verify_ctx;
            let tts_results: Vec<_> = futures::stream::iter(texts.iter())
                .map(|text| async move {
                    let v = crate::audio_verification::verify_with_google_tts(
                        v_ctx,
                        "google-tts",
                        text,
                        voice,
                        api_key_ref,
                    )
                    .await?;
                    anyhow::Ok(v)
                })
                .buffered(4)
                .try_collect()
                .await?;
            let tts_pass = tts_results.iter().filter(|v| v.passed()).count();
            println!(
                "  Google TTS: {tts_pass}/{} passed verification",
                tts_results.len()
            );
            // Append to all_results so they show up in the diagnostic log alongside
            // the human attempts. Don't push to `failures` (we don't drop synthetic
            // audio from the dataset — there's no dataset role for it).
            all_results.extend(tts_results);
        }
    } else if std::env::var("GOOGLE_CLOUD_API_KEY").is_err() {
        log::info!("Skipping Google TTS fallback (GOOGLE_CLOUD_API_KEY not set in env)");
    }

    if !all_results.is_empty() {
        if let Some(parent) = all_results_log.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create directory for {}",
                    all_results_log.display()
                )
            })?;
        }
        let mut f = File::create(all_results_log).with_context(|| {
            format!(
                "Failed to create audio verification log file {}",
                all_results_log.display()
            )
        })?;
        for v in &all_results {
            let json = serde_json::to_string(v)
                .context("Failed to serialize audio verification result")?;
            writeln!(f, "{json}").context("Failed to write audio verification result")?;
        }
        let pass_total = all_results.iter().filter(|v| v.passed()).count();
        println!(
            "Wrote {} audio verification result(s) to {} ({} passed, {} rejected)",
            all_results.len(),
            all_results_log.display(),
            pass_total,
            all_results.len() - pass_total,
        );
    }

    if !failures.is_empty() {
        if let Some(parent) = failures_log.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create directory for {}", failures_log.display())
            })?;
        }
        let mut f = File::create(failures_log).with_context(|| {
            format!(
                "Failed to create audio verification failures file {}",
                failures_log.display()
            )
        })?;
        for v in &failures {
            let json = serde_json::to_string(v)
                .context("Failed to serialize audio verification failure")?;
            writeln!(f, "{json}").context("Failed to write audio verification failure")?;
        }
        println!(
            "Wrote {} audio verification failure(s) to {} for manual review",
            failures.len(),
            failures_log.display()
        );
    }

    Ok(out)
}

/// Encode a .wav file to OGG/Opus by piping ffmpeg's stdout.
///
/// Applies EBU R128 loudness normalization (`loudnorm`) so clips from different
/// contributors land at a consistent perceived volume. -16 LUFS is the Apple
/// Podcasts standard for spoken-word content; quiet recordings get boosted to
/// match, well-recorded ones are left roughly unchanged.
fn encode_wav_to_opus(wav_path: &Path) -> anyhow::Result<Vec<u8>> {
    let output = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(wav_path)
        .args([
            "-af",
            "loudnorm=I=-16:TP=-1.5:LRA=11",
            "-c:a",
            "libopus",
            "-b:a",
            "32k",
            "-application",
            "voip",
            "-f",
            "ogg",
            "pipe:1",
        ])
        .output()
        .context("failed to invoke ffmpeg (is it installed?)")?;
    if !output.status.success() {
        anyhow::bail!(
            "ffmpeg failed for {}: {}",
            wav_path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if !output.stdout.starts_with(b"OggS") {
        anyhow::bail!(
            "ffmpeg produced non-OGG output for {} ({} bytes, magic={:?})",
            wav_path.display(),
            output.stdout.len(),
            &output.stdout.iter().take(4).copied().collect::<Vec<_>>()
        );
    }
    Ok(output.stdout)
}
