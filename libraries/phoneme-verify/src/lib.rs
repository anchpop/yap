//! Verify human-recorded audio against ground-truth IPA using a wav2vec2
//! phoneme model hosted on Modal.
//!
//! For each clip we:
//! 1. Hash the raw .wav bytes (xxh3_64) and check the shared cache store at
//!    `wav2vec2/<WAV2VEC2_CACHE_VERSION>/<hash>` — same caching philosophy
//!    as `translate.rs` and the tysm chat clients (hash → response). The
//!    model/decoder version is part of the key so a model swap can't
//!    silently reuse stale predictions.
//! 2. On cache miss, decode WAV → f32 mono 16kHz via ffmpeg and POST to the
//!    same Modal endpoint the AI backend uses
//!    (`yap-ai-backend/src/main.rs::get_phonemes_from_modal`), then persist
//!    the response.
//! 3. Strip suprasegmental markers from the model's predicted tokens and
//!    diff against the ground-truth IPA assembled from
//!    `word_to_pronunciation`. Ground truth has no suprasegmentals yet, so
//!    we normalize defensively on both sides.
//! 4. Report a [`ClipVerification`] with the verdict; caller decides whether
//!    to drop the clip and/or log it to the per-language failures jsonl.
//!
//! The Modal endpoint is configured via `WAV2VEC2_ENDPOINT_URL` (same env var
//! as the AI backend) with a default to the prod URL.

use anyhow::{Context, Result};
use base64::Engine;
use language_utils::{Language, PhonemeLabelSource};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use xxhash_rust::xxh3::xxh3_64;

pub mod ctc;
pub use ctc::{FrameMatrix, FrameMatrixPayload, TargetScore};

static MODAL_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("WAV2VEC2_ENDPOINT_URL").unwrap_or_else(|_| {
        "https://anchpop--wav2vec2-phoneme-wav2vec2phoneme-predict.modal.run".to_string()
    })
});

/// Bump this whenever the underlying Modal model OR the decoding strategy
/// changes — the cache is partitioned by this string so old entries don't
/// silently get reused with a new model. Format: `<repo>@<revision>__<decoder>`,
/// and the revision must match `MODEL_REVISION` in
/// `modal-envs/wav2vec2_phoneme.py`, since that's what production serves. For
/// ad-hoc model comparisons the eval harness overrides this per-run via
/// `WAV2VEC2_CACHE_VERSION_OVERRIDE`, so this const only governs the default
/// (production) cache partition.
///
/// `edcbbbf43a7f` is the retrain that added the F0-capable acoustic
/// side-channel (log-mel widened to 128 bins / 1024-point window, plus a
/// 64-dim low-band spectrogram over the F0 range) and folded 3,414
/// transcript-verified movie clips into the corpus. Same 392-token vocab as
/// the previous pin, but every prediction moves, so the whole cache partition
/// turns over — expect a full recompute on the next run.
const WAV2VEC2_CACHE_VERSION: &str = "anchpop_lexide-pronunciation@edcbbbf43a7f__greedy_v1";

/// The cache partition production predictions live under — what a caller
/// should record as provenance for anything derived from them.
pub fn production_cache_version() -> String {
    std::env::var("WAV2VEC2_CACHE_VERSION_OVERRIDE")
        .unwrap_or_else(|_| WAV2VEC2_CACHE_VERSION.to_string())
}

#[derive(Debug, Clone, Deserialize)]
struct ModalResponse {
    phonemes: Vec<ModalPhoneme>,
    /// The deploy marker the serving container reports. Checked against
    /// `WAV2VEC2_EXPECTED_DEPLOY_MARKER` (when set, by the eval harness) so a
    /// stale/contaminated container's predictions are rejected before caching.
    /// Absent for older endpoints / production, where the check is a no-op.
    #[serde(default)]
    deploy_marker: Option<String>,
    /// Present when the request asked for `return_frame_matrix`.
    #[serde(default)]
    frame_matrix: Option<FrameMatrixPayload>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ModalPhoneme {
    phoneme: String,
    #[serde(default)]
    confidence: f64,
    /// Top-k alternatives from the model. The chosen phoneme is normally
    /// the first entry, but we re-sort defensively at load time.
    #[serde(default)]
    top_k: Vec<RawPhonemeAlt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawPhonemeAlt {
    phoneme: String,
    probability: f64,
}

/// The on-disk cache payload. Stores both the raw chosen phonemes and the
/// per-position top-k alternatives so we can report probabilities for the
/// expected and predicted phonemes when a clip fails verification.
///
/// `top_k` is required: cache entries written before this field existed
/// fail to deserialize, which we handle by transparently re-fetching.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPrediction {
    raw_phonemes: Vec<String>,
    /// Parallel to `raw_phonemes`. `top_k[i]` is the list of (phoneme,
    /// probability) alternatives for position `i`, sorted by probability
    /// descending.
    top_k: Vec<Vec<RawPhonemeAlt>>,
}

/// One step of the optimal alignment between predicted and expected phoneme
/// sequences. Read in order, the ops reconstruct both sequences and show
/// exactly where they disagree.
///
/// `Sub`/`Extra` carry probabilities sourced from the model's top-k at the
/// predicted position. `expected_prob` is `None` when the expected phoneme
/// wasn't in the model's top-k for that position — i.e. the model
/// effectively assigned zero probability to the correct answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AlignmentOp {
    /// Same phoneme on both sides.
    Match { phoneme: String, probability: f64 },
    /// Different phoneme — predicted has one thing, expected has another.
    Sub {
        expected: String,
        predicted: String,
        predicted_prob: f64,
        expected_prob: Option<f64>,
    },
    /// Predicted has a phoneme the expected sequence doesn't.
    Extra {
        predicted: String,
        predicted_prob: f64,
    },
    /// Expected has a phoneme the model didn't output. We have no model
    /// position for this gap, so no probability is available.
    Missing { expected: String },
}

/// Outcome of verifying one clip. `failure_reason` is `None` iff the clip
/// passed and should be kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipVerification {
    pub actor: String,
    pub text: String,
    pub wav_path: String,
    /// Raw phoneme tokens straight from the model.
    pub predicted_raw: Vec<String>,
    /// Phonemes after stripping suprasegmental markers.
    pub predicted_normalized: Vec<String>,
    /// Ground-truth phonemes assembled from `word_to_pronunciation`. `None`
    /// when some word in `text` isn't in our pronunciation map. When a word
    /// has alternate accepted pronunciations, this is the variant that
    /// matched the model's prediction *most closely* — we pass if any
    /// variant is within threshold, and report the closest one for the
    /// alignment.
    pub expected: Option<Vec<String>>,
    /// Number of alternate (word, variant) combinations considered for
    /// `expected`. 1 when no variants exist; >1 when at least one word in
    /// the phrase has accepted alternates.
    #[serde(default)]
    pub variants_considered: usize,
    pub edit_distance: Option<usize>,
    /// `edit_distance / max(len(predicted), len(expected))`, `None` when
    /// `expected` is None.
    pub edit_distance_pct: Option<f64>,
    /// Step-by-step alignment between predicted and expected. `None` when
    /// `expected` is None.
    pub alignment: Option<Vec<AlignmentOp>>,
    /// `None` if the clip passed the threshold.
    pub failure_reason: Option<String>,
}

impl ClipVerification {
    pub fn passed(&self) -> bool {
        self.failure_reason.is_none()
    }
}

pub struct VerifyContext<'a> {
    pub http: &'a reqwest::Client,
    /// The shared cache store; predictions live under `wav2vec2/{version}/{hash}`.
    store: osmo::Store,
    /// Partitions predictions by (model, decoder) as part of the cache key.
    cache_version: String,
    /// word (lowercase) → accepted IPA pronunciations (main + alternates).
    /// The verifier passes a clip if the model's prediction is within
    /// threshold of *any* of these variants — alternates exist because
    /// wikipron lists multiple valid pronunciations for many French words
    /// (e.g. `mes` is both /me/ and /mɛ/) and our LLM picks one
    /// arbitrarily for "main"; we shouldn't penalize audio that matches a
    /// documented alternate.
    pub word_to_pronunciation: &'a HashMap<String, language_utils::Pronunciations>,
    /// Reject clips whose `edit_distance_pct` exceeds this fraction. 0.3
    /// (30%) is a starting threshold; tune via the env var
    /// `AUDIO_VERIFY_THRESHOLD`.
    pub mismatch_threshold: f64,
    /// Target language — drives per-language phoneme canonicalization (e.g.
    /// collapsing r↔ʁ for French).
    pub target_language: Language,
    /// When set (`WAV2VEC2_EXPECTED_DEPLOY_MARKER`, by the eval harness), every
    /// endpoint response must report this exact deploy marker or we bail rather
    /// than cache a possibly-contaminated prediction. `None` in production.
    expected_deploy_marker: Option<String>,
}

impl<'a> VerifyContext<'a> {
    /// Production / env-driven constructor. The cache version partitions
    /// predictions by (model, decoder); the compile-time const is the default,
    /// overridable via `WAV2VEC2_CACHE_VERSION_OVERRIDE`. Threshold and the
    /// expected deploy marker likewise come from env.
    pub fn new(
        http: &'a reqwest::Client,
        store: osmo::Store,
        word_to_pronunciation: &'a HashMap<String, language_utils::Pronunciations>,
        target_language: Language,
    ) -> Result<Self> {
        let version = std::env::var("WAV2VEC2_CACHE_VERSION_OVERRIDE")
            .unwrap_or_else(|_| WAV2VEC2_CACHE_VERSION.to_string());
        let threshold = std::env::var("AUDIO_VERIFY_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.3);
        let expected_deploy_marker = std::env::var("WAV2VEC2_EXPECTED_DEPLOY_MARKER")
            .ok()
            .filter(|s| !s.is_empty());
        Self::with_overrides(
            http,
            store,
            word_to_pronunciation,
            target_language,
            version,
            threshold,
            expected_deploy_marker,
        )
    }

    /// Explicit constructor for in-process callers (the compare-audio-models
    /// tool) that drive many models in one run and need a distinct cache
    /// partition + expected deploy marker per model — values that env vars
    /// can't carry safely when mutated mid-run across async tasks.
    #[allow(clippy::too_many_arguments)]
    pub fn with_overrides(
        http: &'a reqwest::Client,
        store: osmo::Store,
        word_to_pronunciation: &'a HashMap<String, language_utils::Pronunciations>,
        target_language: Language,
        cache_version: String,
        mismatch_threshold: f64,
        expected_deploy_marker: Option<String>,
    ) -> Result<Self> {
        // Fail closed on the label source. The model is trained on labels from
        // exactly one G2P engine per language; scoring against a different
        // one silently compares incompatible phoneme inventories, and every
        // downstream number still *looks* fine. Hindi is the worked example:
        // scored against espeak `hi` it measured as our worst language by a
        // wide margin, because its real labels come from `schwa-stress-hin`,
        // which emits aspiration bound to its consonant rather than as the
        // standalone `ʰ` espeak produces.
        //
        // Refusing here rather than in the caller matters, because the
        // fallback is not "no targets" — `ground_truth_phoneme_variants`
        // would quietly fall back to wikipron, which is a *third* inventory.
        match target_language.phoneme_label_source() {
            PhonemeLabelSource::Espeak(_) => {}
            PhonemeLabelSource::Backend(provider) => anyhow::bail!(
                "{:?} phoneme labels come from the {provider} backend, which we \
                 don't run yet — refusing to verify audio against espeak or \
                 wikipron targets. See lexide's PHONEME_BACKENDS.md.",
                target_language
            ),
            PhonemeLabelSource::Unvalidated => anyhow::bail!(
                "{:?} has no validated phoneme label source — refusing to \
                 verify audio against an unchecked reference. See lexide's \
                 PHONEME_BACKENDS.md.",
                target_language
            ),
        }
        Ok(Self {
            http,
            store,
            cache_version,
            word_to_pronunciation,
            mismatch_threshold,
            target_language,
            expected_deploy_marker,
        })
    }
}

/// Compute the accepted phoneme sequence(s) for a clip.
///
/// If `override_transcription` is `Some`, uses that single whitespace-
/// separated IPA string as the sole ground truth — encodes a speaker's
/// connected-speech realization that may differ from citation form (e.g.
/// French `de` produced as /dø/). Otherwise derives from wikipron's
/// per-word variants (cross-product, capped at MAX_VARIANT_COMBINATIONS)
/// plus the espeak phrase-level rendering.
///
/// Returns `None` only when no source produced any candidates — e.g. a
/// word missing from wikipron AND a language with no espeak support.
pub fn expected_phoneme_variants(
    ctx: &VerifyContext<'_>,
    text: &str,
    override_transcription: Option<&str>,
) -> Option<Vec<Vec<String>>> {
    if let Some(s) = override_transcription {
        let normalized: Vec<String> = s
            .split_whitespace()
            .filter_map(|t| normalize_phoneme(t, ctx.target_language))
            .collect();
        return if normalized.is_empty() {
            None
        } else {
            Some(vec![normalized])
        };
    }
    ground_truth_phoneme_variants(text, ctx.word_to_pronunciation, ctx.target_language)
}

/// Verify a clip against an explicit accepted-phoneme-sequence set.
///
/// The caller is responsible for computing `expected` (see
/// [`expected_phoneme_variants`]). Splitting derivation from verification
/// keeps this function single-purpose: feed it audio + accepted
/// sequences, get back a verdict.
pub async fn verify_clip(
    ctx: &VerifyContext<'_>,
    actor: &str,
    text: &str,
    wav_path: &Path,
    expected: Option<Vec<Vec<String>>>,
) -> Result<ClipVerification> {
    let wav_bytes = std::fs::read(wav_path)
        .with_context(|| format!("Failed to read wav {}", wav_path.display()))?;
    verify_clip_bytes(
        ctx,
        actor,
        text,
        &wav_path.display().to_string(),
        &wav_bytes,
        expected,
    )
    .await
}

/// Bytes-based variant of [`verify_clip`] for callers that already have the
/// audio in memory (e.g. Google TTS output, where we don't need a file at
/// all). `source_label` is recorded in the result purely for diagnostics.
pub async fn verify_clip_bytes(
    ctx: &VerifyContext<'_>,
    actor: &str,
    text: &str,
    source_label: &str,
    audio_bytes: &[u8],
    expected: Option<Vec<Vec<String>>>,
) -> Result<ClipVerification> {
    // Early defect check: if the audio is too quiet or truncated, the
    // wav2vec2 prediction is unreliable (and on near-silent input the
    // model may still emit *something*, which gives misleading "matches
    // expected" results). Skip verification entirely and surface the
    // defect as the failure reason. Mirrors the same check applied to
    // Google TTS output in `verify_with_google_tts`.
    if let Ok(samples) = decode_wav_to_f32(audio_bytes)
        && let Some(defect) = google_tts::samples_defect(&samples, MODAL_SAMPLE_RATE)
    {
        return Ok(ClipVerification {
            actor: actor.to_string(),
            text: text.to_string(),
            wav_path: source_label.to_string(),
            predicted_raw: Vec::new(),
            predicted_normalized: Vec::new(),
            expected: None,
            variants_considered: 0,
            edit_distance: None,
            edit_distance_pct: None,
            alignment: None,
            failure_reason: Some(format!("audio defect: {defect}")),
        });
    }

    let (raw_phonemes, raw_top_k) = predict_phonemes(ctx, audio_bytes)
        .await
        .with_context(|| format!("Failed to predict phonemes for {source_label}"))?;
    let (predicted_normalized, predicted_top_k) =
        normalize_with_topk(&raw_phonemes, &raw_top_k, ctx.target_language);

    // Caller supplies the accepted phoneme sequence(s) — see
    // [`expected_phoneme_variants`] for how to build them from wikipron +
    // espeak (or a per-clip override).
    let variant_candidates = expected;

    let (
        expected,
        variants_considered,
        edit_distance,
        edit_distance_pct,
        alignment,
        failure_reason,
    ) = match variant_candidates {
        // No ground truth available — both wikipron and espeak came up
        // empty. Reject the clip rather than silently passing: shipping
        // unverified audio is worse than dropping a clip we can fix by
        // adding a wikipron entry or extending espeak coverage.
        None => (
            None,
            0,
            None,
            None,
            None,
            Some(
                "no ground truth available (word missing from wikipron and espeak unsupported \
                 for this language)"
                    .to_string(),
            ),
        ),
        Some(variants) => {
            let n_variants = variants.len();
            // Score each variant by edit distance against the prediction;
            // pick the closest one. Ties broken by variant order (main first).
            let mut best: Option<(usize, Vec<String>, Vec<AlignmentOp>)> = None;
            for variant in variants {
                let (dist, ops) = align(&predicted_normalized, &predicted_top_k, &variant);
                if best.as_ref().is_none_or(|(d, _, _)| dist < *d) {
                    best = Some((dist, variant, ops));
                }
            }
            let (dist, exp, ops) = best.unwrap();
            let max_len = predicted_normalized.len().max(exp.len()).max(1);
            let pct = dist as f64 / max_len as f64;
            let reason = if predicted_normalized.is_empty() {
                Some("model returned no phonemes".to_string())
            } else if pct > ctx.mismatch_threshold {
                Some(format!(
                    "phoneme mismatch ({dist} edits over max-len {max_len} = {:.0}%, \
                         best of {n_variants} variant(s))",
                    pct * 100.0
                ))
            } else {
                None
            };
            (
                Some(exp),
                n_variants,
                Some(dist),
                Some(pct),
                Some(ops),
                reason,
            )
        }
    };

    Ok(ClipVerification {
        actor: actor.to_string(),
        text: text.to_string(),
        wav_path: source_label.to_string(),
        predicted_raw: raw_phonemes,
        predicted_normalized,
        expected,
        variants_considered,
        edit_distance,
        edit_distance_pct,
        alignment,
        failure_reason,
    })
}

/// Sample rate we always send to Modal — matches wav2vec2's training rate.
const MODAL_SAMPLE_RATE: u32 = 16_000;

/// Minimum samples we send to Modal. wav2vec2's convolutional encoder needs
/// roughly one full stride window of context (~320 samples at 16 kHz, but
/// in practice the Modal endpoint 500s on anything below several hundred
/// ms), so we pad with leading/trailing silence to stay safely above that
/// floor. 0.6 s is comfortably past every minimum we've observed.
const MODAL_MIN_SAMPLES: usize = (MODAL_SAMPLE_RATE as usize * 6) / 10;

/// How many alternatives we ask Modal for at each position. Larger = more
/// chance of catching the correct phoneme in the top-k for failure
/// analysis; trades off cache size linearly. 10 is comfortable for French.
const MODAL_TOP_K: usize = 10;

/// How many times to attempt a single Modal prediction before giving up.
/// Transient failures (cold-start 408s, rate limits, 5xx) are retried with a
/// linear backoff; a non-transient status fails immediately.
const MODAL_MAX_ATTEMPTS: usize = 5;

/// Whether an HTTP status from the Modal endpoint is worth retrying.
fn is_transient_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

/// POST one request to the endpoint, retrying transient failures, and
/// refuse any response from a container whose deploy marker is not the
/// one expected — so nothing from a stale/contaminated container is
/// ever cached under the wrong model's key.
async fn post_modal(ctx: &VerifyContext<'_>, payload: serde_json::Value) -> Result<ModalResponse> {
    // Retry transient endpoint failures — cold-start timeouts (408), rate
    // limits (429), and 5xx — which are otherwise fatal to a long run. A 408
    // typically means the container was mid-cold-start; a short backoff lets it
    // finish and the retry lands on the now-warm container.
    let modal: ModalResponse = {
        let mut last_err: Option<anyhow::Error> = None;
        let mut got: Option<ModalResponse> = None;
        for attempt in 1..=MODAL_MAX_ATTEMPTS {
            match ctx
                .http
                .post(MODAL_URL.as_str())
                .json(&payload)
                .send()
                .await
            {
                Err(e) => {
                    last_err = Some(anyhow::Error::new(e).context("Modal request transport error"));
                }
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<ModalResponse>().await {
                            Ok(m) => {
                                got = Some(m);
                                break;
                            }
                            Err(e) => {
                                last_err = Some(
                                    anyhow::Error::new(e)
                                        .context("Failed to parse Modal wav2vec2 response"),
                                )
                            }
                        }
                    } else if is_transient_status(status) {
                        let body = response.text().await.unwrap_or_default();
                        last_err =
                            Some(anyhow::anyhow!("Modal wav2vec2 transient {status}: {body}"));
                    } else {
                        // Non-retryable (e.g. 400): fail immediately.
                        let body = response.text().await.unwrap_or_default();
                        anyhow::bail!("Modal wav2vec2 error ({status}): {body}");
                    }
                }
            }
            if attempt < MODAL_MAX_ATTEMPTS {
                let delay = std::time::Duration::from_secs(5 * attempt as u64);
                log::warn!(
                    "Modal call failed (attempt {attempt}/{MODAL_MAX_ATTEMPTS}), retrying in {}s",
                    delay.as_secs()
                );
                tokio::time::sleep(delay).await;
            }
        }
        match got {
            Some(m) => m,
            None => {
                return Err(last_err
                    .unwrap_or_else(|| anyhow::anyhow!("Modal call failed"))
                    .context(format!(
                        "Modal wav2vec2 endpoint failed after {MODAL_MAX_ATTEMPTS} attempts"
                    )));
            }
        }
    };

    // Per-request freshness check: the one-shot marker_only probe only proves
    // the *first* request hit a fresh container. Verifying the marker on every
    // response guarantees no later request was routed to a stale/contaminated
    // warm container and silently cached under the wrong model's key.
    if let Some(expected) = &ctx.expected_deploy_marker
        && modal.deploy_marker.as_deref() != Some(expected.as_str())
    {
        anyhow::bail!(
            "deploy-marker mismatch: endpoint reported {:?}, expected {expected:?} — \
             refusing to cache a possibly-contaminated prediction",
            modal.deploy_marker
        );
    }
    Ok(modal)
}

async fn predict_phonemes(
    ctx: &VerifyContext<'_>,
    wav_bytes: &[u8],
) -> Result<(Vec<String>, Vec<Vec<RawPhonemeAlt>>)> {
    let hash = xxh3_64(wav_bytes);
    let cache_key = format!("wav2vec2/{}/{hash:016x}", ctx.cache_version);

    if let Some(contents) = ctx.store.read(&cache_key).await
        && let Ok(cached) = serde_json::from_slice::<CachedPrediction>(&contents)
    {
        return Ok((cached.raw_phonemes, cached.top_k));
    }

    if cache_only() {
        anyhow::bail!(
            "wav2vec2 cache miss for {hash:016x}; cache-only mode is enabled. \
             Run without --cache-only to populate the cache."
        );
    }

    let samples =
        decode_wav_to_f32(wav_bytes).context("Failed to decode WAV to f32 samples via ffmpeg")?;
    let samples = pad_to_min_length(samples, MODAL_MIN_SAMPLES);
    let payload = serde_json::json!({
        "audio": samples,
        "sample_rate": MODAL_SAMPLE_RATE,
        "top_k": MODAL_TOP_K,
    });
    let modal = post_modal(ctx, payload).await?;

    let raw_phonemes: Vec<String> = modal.phonemes.iter().map(|p| p.phoneme.clone()).collect();
    let top_k: Vec<Vec<RawPhonemeAlt>> = modal
        .phonemes
        .into_iter()
        .map(|p| {
            // Defensively sort by probability desc; Modal returns them sorted
            // already, but we depend on that ordering.
            let mut alts = p.top_k;
            alts.sort_by(|a, b| {
                b.probability
                    .partial_cmp(&a.probability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            alts
        })
        .collect();

    let to_write = CachedPrediction {
        raw_phonemes: raw_phonemes.clone(),
        top_k: top_k.clone(),
    };
    let serialized =
        serde_json::to_string(&to_write).context("Failed to serialize cache payload")?;
    ctx.store
        .write(&cache_key, serialized.as_bytes())
        .await
        .with_context(|| format!("Failed to write cache entry {cache_key}"))?;

    Ok((raw_phonemes, top_k))
}

/// The model's per-frame log-prob matrix for a clip, from the cache or the
/// endpoint. Cached under its own partition (`wav2vec2-frames/…`), keyed by
/// the WAV bytes like predictions are; the compressed payload is stored as
/// shipped, so a cache entry is ~24 KB per audio-second.
pub async fn frame_matrix(ctx: &VerifyContext<'_>, wav_bytes: &[u8]) -> Result<FrameMatrix> {
    let hash = xxh3_64(wav_bytes);
    let cache_key = format!("wav2vec2-frames/{}/{hash:016x}", ctx.cache_version);
    if let Some(contents) = ctx.store.read(&cache_key).await
        && let Ok(payload) = serde_json::from_slice::<FrameMatrixPayload>(&contents)
    {
        return FrameMatrix::decode(&payload);
    }
    if cache_only() {
        anyhow::bail!(
            "wav2vec2 frame-matrix cache miss for {hash:016x}; cache-only mode is enabled"
        );
    }
    let samples =
        decode_wav_to_f32(wav_bytes).context("Failed to decode WAV to f32 samples via ffmpeg")?;
    let samples = pad_to_min_length(samples, MODAL_MIN_SAMPLES);
    let payload = serde_json::json!({
        "audio": samples,
        "sample_rate": MODAL_SAMPLE_RATE,
        "top_k": 1,
        "return_frame_matrix": true,
    });
    let modal = post_modal(ctx, payload).await?;
    let Some(payload) = modal.frame_matrix else {
        anyhow::bail!(
            "endpoint returned no frame matrix (does this deploy support return_frame_matrix?)"
        );
    };
    let serialized = serde_json::to_string(&payload).context("serializing frame matrix")?;
    ctx.store
        .write(&cache_key, serialized.as_bytes())
        .await
        .with_context(|| format!("Failed to write cache entry {cache_key}"))?;
    FrameMatrix::decode(&payload)
}

/// Pad `samples` symmetrically with zeros to reach at least `min_len`.
/// wav2vec2 normally sees speech surrounded by silence, so leading/trailing
/// zero-padding is a no-op for phoneme prediction on the speech we *do*
/// care about — and it lifts very short clips (e.g. 0.2 s synthetic-TTS
/// renders of single syllables) above the Modal endpoint's minimum-length
/// floor.
fn pad_to_min_length(samples: Vec<f32>, min_len: usize) -> Vec<f32> {
    if samples.len() >= min_len {
        return samples;
    }
    let needed = min_len - samples.len();
    let lead = needed / 2;
    let trail = needed - lead;
    let mut padded = Vec::with_capacity(min_len);
    padded.resize(lead, 0.0);
    padded.extend_from_slice(&samples);
    padded.resize(padded.len() + trail, 0.0);
    padded
}

/// Decode WAV bytes to mono f32 samples at 16 kHz by piping through ffmpeg.
/// 16 kHz is the standard rate wav2vec2 phoneme models expect.
fn decode_wav_to_f32(wav_bytes: &[u8]) -> Result<Vec<f32>> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-loglevel",
            "error",
            "-i",
            "pipe:0",
            "-f",
            "f32le",
            "-ar",
            "16000",
            "-ac",
            "1",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn ffmpeg for WAV decoding")?;

    // Write stdin from its own thread: ffmpeg streams output while it
    // still has input left to read, so once the input is larger than the
    // OS pipe buffers, writing it all before draining stdout deadlocks —
    // ffmpeg blocks writing stdout, we block writing stdin. Short clips
    // never hit this; a 12-second movie cue does. Same pattern as
    // `subtitle-corpus`'s `encode_opus`.
    let mut stdin = child.stdin.take().context("ffmpeg stdin not captured")?;
    let owned_bytes = wav_bytes.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&owned_bytes));

    let output = child
        .wait_with_output()
        .context("Failed to wait for ffmpeg")?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("ffmpeg stdin writer thread panicked"))?
        .context("Failed to write WAV bytes to ffmpeg stdin")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg decode failed ({}): {stderr}", output.status);
    }

    if output.stdout.len() % 4 != 0 {
        anyhow::bail!(
            "ffmpeg output is not a multiple of 4 bytes ({} bytes)",
            output.stdout.len()
        );
    }
    Ok(output
        .stdout
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// Normalize a single IPA token into a canonical comparable form. Returns
/// `None` for tokens that consist entirely of non-phonemic markers.
///
/// Three layers of cleanup, applied in order:
///
/// 1. **Universal stripping** — remove characters that aren't phonemic in
///    any language: suprasegmental stress (`ˈ`/`ˌ`), syllable boundary
///    (`.`), length marks (`ː`/`ˑ`), the liaison/elision marker (`‿`), and
///    ASCII digits (the multilingual wav2vec2 model leaks Mandarin tone
///    numbers like `y5`, `i5`, `a5` into French output).
/// 2. **Internal whitespace strip** — `f a ɪ` is already split on
///    whitespace by the caller, but defensive in case a token slipped
///    through with embedded whitespace.
/// 3. **Per-language canonicalization** — collapses phoneme equivalence
///    classes so model conventions and ground-truth conventions agree:
///    for French, `{r, ʀ}` collapse to `ʁ`, `{ts, tɕ, tɕh}` to `t`,
///    `ɥ → y` (the model doesn't reliably distinguish the rounded palatal
///    approximant from `y`).
///
/// Combining diacritics inside the phoneme (e.g. the tilde on `ã`) are NOT
/// touched — those are phonemic.
pub fn normalize_phoneme(token: &str, language: Language) -> Option<String> {
    let stripped: String = token
        .chars()
        .filter(|c| {
            !matches!(*c, 'ˈ' | 'ˌ' | '.' | 'ː' | 'ˑ' | '‿')
                && !c.is_ascii_digit()
                && !c.is_whitespace()
        })
        .collect();
    if stripped.is_empty() {
        return None;
    }
    Some(canonicalize_for_language(&stripped, language))
}

/// Map a phoneme token onto its canonical form for the given target
/// language. Symmetric across predicted/expected — when both sides go
/// through this, equivalence-class members compare equal.
fn canonicalize_for_language(token: &str, language: Language) -> String {
    match language {
        Language::French => match token {
            // R variants: ground truth uses ʁ (uvular fricative); the
            // multilingual model emits any of: r (alveolar trill, common
            // across many languages), ʀ (uvular trill, French stage variant),
            // ɾ (alveolar tap, Spanish/Italian-flavored), x (voiceless velar
            // fricative, sometimes emitted for the devoiced uvular allophone
            // /χ/ that occurs phrase-finally in French). All collapse to ʁ.
            "r" | "ʀ" | "ʁ" | "ɾ" | "x" => "ʁ".to_string(),
            // /t/ affricate variants observed in the model output for
            // French. Not phonemic in French; collapse to plain /t/.
            "ts" | "tɕ" | "tɕh" | "tɕʰ" => "t".to_string(),
            // The model conflates the rounded palatal approximant with /y/.
            "ɥ" => "y".to_string(),
            // Front /a/ vs back /ɑ/ is no longer phonemically distinguished
            // in modern Parisian French; both wikipron and the model use
            // them inconsistently. Collapse to a.
            "ɑ" | "a" => "a".to_string(),
            _ => token.to_string(),
        },
        _ => token.to_string(),
    }
}

/// Hard cap on combinatorial expansion of word-level pronunciation
/// variants into phrase-level candidates. A phrase with 5 words each
/// having 2 variants generates 32 phrase candidates; we don't want to
/// expand a long sentence with many multi-variant words into thousands.
const MAX_VARIANT_COMBINATIONS: usize = 16;

/// Build the set of accepted phoneme sequences for a target-language
/// phrase. Each word in the phrase contributes its main pronunciation
/// plus any accepted alternates; the result is the (capped) cross
/// product across words.
///
/// Returns `None` only if we couldn't produce *any* candidate — i.e.
/// every word in the phrase is missing from wikipron AND espeak doesn't
/// support this language. As long as one source (wikipron cross-product
/// OR espeak) produces something, we return it.
///
/// When the wikipron cross product would exceed
/// `MAX_VARIANT_COMBINATIONS`, we fall back to just the main-only
/// variant: better to under-accept than to spend exponential time
/// enumerating a long sentence.
///
/// The espeak phrase-level IPA, when available, is *always* added as an
/// extra candidate independent of the wikipron path. Espeak applies
/// connected-speech rules (liaison, elision, reduction) that the
/// word-by-word wikipron expansion misses. The motivating example:
/// "on est" — wikipron has "on"=`/ɔ̃/` or `/ɔ.n‿/` and "est"=`/ɛ/`, but
/// in actual speech (and TTS) it surfaces as `/ɔ̃ n ɛ/` (nasal vowel
/// preserved + liaison /n/) which neither combination produces. As a
/// side effect, espeak also rescues phrases that include words missing
/// from wikipron (`est-ce que`, `peut-être`) from silently being skipped.
fn ground_truth_phoneme_variants(
    text: &str,
    word_to_pronunciation: &HashMap<String, language_utils::Pronunciations>,
    language: Language,
) -> Option<Vec<Vec<String>>> {
    // Collect per-word phoneme-sequence variants in phrase order.
    // `wikipron_complete` flips to false the moment any word is missing
    // from the dictionary; we then skip the cross-product and rely on
    // espeak (if available) as the sole ground truth.
    let mut per_word: Vec<Vec<Vec<String>>> = Vec::new();
    let mut wikipron_complete = true;
    for word in text.split(|c: char| {
        c.is_whitespace() || (!c.is_alphabetic() && c != '\'' && c != '-' && c != 'ʼ')
    }) {
        let cleaned = word
            .trim_matches(|c: char| !c.is_alphabetic())
            .to_lowercase();
        if cleaned.is_empty() {
            continue;
        }
        let Some(accepted) = word_to_pronunciation.get(&cleaned) else {
            wikipron_complete = false;
            continue;
        };
        let word_variants: Vec<Vec<String>> = accepted
            .all()
            .map(|ipa| {
                ipa.split_whitespace()
                    .filter_map(|p| normalize_phoneme(p, language))
                    .collect::<Vec<String>>()
            })
            // Distinct sequences only — after normalization, different raw
            // wikipron entries can collapse to the same phoneme sequence.
            .fold(Vec::new(), |mut acc, v| {
                if !acc.contains(&v) {
                    acc.push(v);
                }
                acc
            });
        per_word.push(word_variants);
    }

    let mut candidates: Vec<Vec<String>> = if !wikipron_complete || per_word.is_empty() {
        // No wikipron candidates — espeak below is our only shot.
        Vec::new()
    } else {
        let total: usize = per_word.iter().map(|v| v.len().max(1)).product::<usize>();
        if total > MAX_VARIANT_COMBINATIONS {
            // Fall back to main-only.
            let mut out = Vec::with_capacity(per_word.iter().map(|v| v[0].len()).sum::<usize>());
            for word_variants in &per_word {
                out.extend(word_variants[0].iter().cloned());
            }
            vec![out]
        } else {
            // Enumerate the cross product. `acc` accumulates phrase
            // candidates; for each word we re-expand each accumulated
            // candidate against each of that word's variants.
            let mut acc: Vec<Vec<String>> = vec![Vec::new()];
            for word_variants in &per_word {
                let mut next = Vec::with_capacity(acc.len() * word_variants.len());
                for prefix in &acc {
                    for var in word_variants {
                        let mut extended = prefix.clone();
                        extended.extend(var.iter().cloned());
                        next.push(extended);
                    }
                }
                acc = next;
            }
            acc
        }
    };

    // Add the espeak phrase-level variant, if espeak supports this
    // language. Espeak runs in <10 ms; cheap enough to call inline.
    // Failures here are *important*: when espeak silently dies (binary
    // missing, data-path wrong) the verifier loses all phrase-level
    // ground truth for text that wikipron can't decompose — and that's
    // hard to notice unless we surface it. Log the first error per
    // process so a broken environment doesn't disappear into the noise.
    match espeak::phonemize_phrase(text, language) {
        Ok(Some(espeak_raw)) => {
            let espeak_seq: Vec<String> = espeak_raw
                .iter()
                .filter_map(|p| normalize_phoneme(p, language))
                .collect();
            if !espeak_seq.is_empty() && !candidates.contains(&espeak_seq) {
                candidates.push(espeak_seq);
            }
        }
        Ok(None) => {
            // language has no espeak support — expected for languages
            // intentionally excluded (Korean, etc.), no warning needed.
        }
        Err(e) => {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                log::warn!("espeak phonemization failed (first occurrence shown only): {e:#}");
            });
        }
    }

    if candidates.is_empty() {
        None
    } else {
        Some(candidates)
    }
}

/// Normalize the model's raw phoneme output AND the parallel top-k
/// alternatives at the same time. Returns the two lists with matching
/// length, parallel by position.
///
/// At each raw position:
///   * If the chosen phoneme normalizes to `None` (i.e. it's pure
///     suprasegmental), the entire position is dropped — same as the
///     existing `predicted_normalized` behavior.
///   * Otherwise, the position is kept. The top-k alternatives are also
///     normalized, dropped-where-None, and then merged when two distinct
///     raw alternatives normalize to the same form (e.g. raw `r` + `ʁ` →
///     a single `ʁ` entry with summed probability). The merged top-k is
///     re-sorted by probability descending.
fn normalize_with_topk(
    raw_phonemes: &[String],
    raw_top_k: &[Vec<RawPhonemeAlt>],
    language: Language,
) -> (Vec<String>, Vec<Vec<(String, f64)>>) {
    let mut normalized = Vec::with_capacity(raw_phonemes.len());
    let mut normalized_top_k: Vec<Vec<(String, f64)>> = Vec::with_capacity(raw_phonemes.len());

    for (i, raw) in raw_phonemes.iter().enumerate() {
        let Some(norm) = normalize_phoneme(raw, language) else {
            continue;
        };
        normalized.push(norm);

        let mut merged: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        if let Some(alts) = raw_top_k.get(i) {
            for alt in alts {
                if let Some(alt_norm) = normalize_phoneme(&alt.phoneme, language) {
                    *merged.entry(alt_norm).or_insert(0.0) += alt.probability;
                }
            }
        }
        let mut vec: Vec<(String, f64)> = merged.into_iter().collect();
        vec.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        normalized_top_k.push(vec);
    }

    (normalized, normalized_top_k)
}

/// Look up `phoneme`'s probability in a normalized top-k list. Returns
/// `None` when the phoneme wasn't in the top-k — i.e. the model
/// effectively assigned ~0 probability to it.
fn prob_of(phoneme: &str, top_k: &[(String, f64)]) -> Option<f64> {
    top_k
        .iter()
        .find(|(p, _)| p == phoneme)
        .map(|(_, prob)| *prob)
}

/// Levenshtein alignment with per-position probability annotations. Same
/// algorithm as before; ops now carry the model's confidence at the
/// predicted position so the JSONL output can show how close the model
/// was to the correct answer.
fn align(
    predicted: &[String],
    predicted_top_k: &[Vec<(String, f64)>],
    expected: &[String],
) -> (usize, Vec<AlignmentOp>) {
    let (m, n) = (predicted.len(), expected.len());

    // Helper for the chosen phoneme's prob at predicted position i. Falls
    // back to 1.0 if the position has no top-k (shouldn't happen, but
    // defensive — e.g. for callers in tests that pass empty top-k).
    let pred_prob = |i: usize| -> f64 {
        predicted_top_k
            .get(i)
            .and_then(|alts| alts.first())
            .map(|(_, p)| *p)
            .unwrap_or(1.0)
    };
    // Helper for prob of the expected phoneme at predicted position i.
    let exp_prob_at = |i: usize, exp_ph: &str| -> Option<f64> {
        predicted_top_k
            .get(i)
            .and_then(|alts| prob_of(exp_ph, alts))
    };

    // dp[i][j] = min cost to align predicted[..i] with expected[..j].
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if predicted[i - 1] == expected[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    // Backtrack from (m, n) to (0, 0), preferring match > sub > extra/missing
    // on ties so the alignment is readable. `i-1` is the predicted position
    // we're emitting an op for; we use it to look up probabilities.
    let mut ops: Vec<AlignmentOp> = Vec::with_capacity(m.max(n));
    let (mut i, mut j) = (m, n);
    while i > 0 || j > 0 {
        let here = dp[i][j];
        if i > 0 && j > 0 {
            let cost = if predicted[i - 1] == expected[j - 1] {
                0
            } else {
                1
            };
            if dp[i - 1][j - 1] + cost == here {
                if cost == 0 {
                    ops.push(AlignmentOp::Match {
                        phoneme: predicted[i - 1].clone(),
                        probability: pred_prob(i - 1),
                    });
                } else {
                    ops.push(AlignmentOp::Sub {
                        expected: expected[j - 1].clone(),
                        predicted: predicted[i - 1].clone(),
                        predicted_prob: pred_prob(i - 1),
                        expected_prob: exp_prob_at(i - 1, &expected[j - 1]),
                    });
                }
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && dp[i - 1][j] + 1 == here {
            ops.push(AlignmentOp::Extra {
                predicted: predicted[i - 1].clone(),
                predicted_prob: pred_prob(i - 1),
            });
            i -= 1;
            continue;
        }
        // Remaining case: dp[i][j-1] + 1 == here
        ops.push(AlignmentOp::Missing {
            expected: expected[j - 1].clone(),
        });
        j -= 1;
    }
    ops.reverse();
    (dp[m][n], ops)
}

// --- Google-TTS-driven fallback verification ---------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTts {
    /// The original text that produced this audio. Recorded so the cache is
    /// debuggable from filename → text without re-running. `Option` so older
    /// cache entries (written before this field existed) still load.
    #[serde(default)]
    text: Option<String>,
    /// OGG/Opus audio bytes, base64-encoded.
    audio_base64: String,
    attempts: usize,
    /// `passed` indicates the audio passed Google's defect check;
    /// otherwise we returned the last attempt and recorded the defect.
    passed: bool,
    last_defect: Option<String>,
}

/// Voice and language metadata for the Google TTS call. Caller maps their
/// own Language enum into this.
#[derive(Debug, Clone, Copy)]
pub struct TtsVoice {
    pub language_code: &'static str,
    pub voice_name: &'static str,
}

/// Map a target language to Google TTS voice metadata. Currently only the
/// languages we ship audio for. Returns `None` for unsupported languages.
pub fn default_voice_for(language: Language) -> Option<TtsVoice> {
    Some(match language {
        Language::French => TtsVoice {
            language_code: "fr-FR",
            voice_name: "fr-FR-Chirp3-HD-Achernar",
        },
        Language::Spanish => TtsVoice {
            language_code: "es-US",
            voice_name: "es-US-Chirp3-HD-Achernar",
        },
        Language::English => TtsVoice {
            language_code: "en-US",
            voice_name: "en-US-Chirp3-HD-Achernar",
        },
        Language::German => TtsVoice {
            language_code: "de-DE",
            voice_name: "de-DE-Chirp3-HD-Achernar",
        },
        Language::Italian => TtsVoice {
            language_code: "it-IT",
            voice_name: "it-IT-Chirp3-HD-Achernar",
        },
        Language::Portuguese => TtsVoice {
            language_code: "pt-BR",
            voice_name: "pt-BR-Chirp3-HD-Achernar",
        },
        Language::Russian => TtsVoice {
            language_code: "ru-RU",
            voice_name: "ru-RU-Chirp3-HD-Aoede",
        },
        Language::Korean => TtsVoice {
            language_code: "ko-KR",
            voice_name: "ko-KR-Chirp3-HD-Achernar",
        },
        _ => return None,
    })
}

/// Call Google TTS for `text` (using `voice`), running the result through
/// the same wav2vec2 verifier we use for human audio. The synthesized audio
/// is cached under `google-tts/{hash}` keyed by (text, voice, speed).
///
/// `actor` is recorded on the resulting [`ClipVerification`] for logging —
/// callers typically pass something like `"google-tts"` so the synthetic
/// verifications are easy to grep out of the all-results log.
///
/// If the Google TTS retry loop hit its limit, this prepends a note to the
/// verification's `failure_reason` so the synthetic audio's outcome is
/// distinguishable from a clean pass-or-fail.
pub async fn verify_with_google_tts(
    ctx: &VerifyContext<'_>,
    actor: &str,
    text: &str,
    voice: TtsVoice,
    google_api_key: &str,
) -> Result<ClipVerification> {
    // Cache key: hash the request inputs that uniquely determine the output.
    // If we ever change voice/speed/text, the cache miss is automatic.
    let speed = 1.0f64;
    let cache_seed = format!(
        "{text}|{}|{}|{speed}",
        voice.language_code, voice.voice_name
    );
    let hash = xxh3_64(cache_seed.as_bytes());
    let cache_key = format!("google-tts/{hash:016x}");

    let (audio_bytes, tts_note) = if let Some(s) = ctx.store.read(&cache_key).await
        && let Ok(cached) = serde_json::from_slice::<CachedTts>(&s)
    {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&cached.audio_base64)
            .context("Cached google-tts audio_base64 was not valid base64")?;
        let note = (!cached.passed).then(|| {
            format!(
                "google-tts hit retry limit after {} attempts ({})",
                cached.attempts,
                cached.last_defect.as_deref().unwrap_or("unknown defect")
            )
        });
        (bytes, note)
    } else {
        if cache_only() {
            anyhow::bail!(
                "google-tts cache miss for {text:?} ({hash:016x}); cache-only mode is enabled"
            );
        }
        let client =
            google_tts::GoogleTtsClient::new(google_api_key.to_string()).with_max_attempts(5);
        let outcome = client
            .synthesize(&google_tts::GoogleTtsRequest {
                text: text.to_string(),
                language_code: voice.language_code.to_string(),
                voice_name: voice.voice_name.to_string(),
                speed,
                is_ssml: false,
            })
            .await
            .with_context(|| format!("Google TTS call failed for {text:?}"))?;
        let (passed, last_defect) = match outcome.status {
            google_tts::TtsStatus::Passed => (true, None),
            google_tts::TtsStatus::HitLimit { last_defect } => {
                (false, Some(last_defect.to_string()))
            }
        };
        let to_cache = CachedTts {
            text: Some(text.to_string()),
            audio_base64: base64::engine::general_purpose::STANDARD.encode(&outcome.audio_bytes),
            attempts: outcome.attempts,
            passed,
            last_defect: last_defect.clone(),
        };
        ctx.store
            .write(
                &cache_key,
                serde_json::to_string(&to_cache)
                    .context("serialize cached_tts")?
                    .as_bytes(),
            )
            .await
            .with_context(|| format!("Failed to write cache entry {cache_key}"))?;
        let note = (!passed).then(|| {
            format!(
                "google-tts hit retry limit after {} attempts ({})",
                outcome.attempts,
                last_defect.unwrap_or_else(|| "unknown defect".to_string())
            )
        });
        (outcome.audio_bytes, note)
    };

    // If the TTS retry loop gave up, skip verification entirely. Running
    // wav2vec2 on near-silent or truncated audio invites the model to
    // hallucinate plausible phonemes (the `pas` case: TTS returned 0.19s
    // of -50 dB audio, we padded to 0.6s with zeros, model emitted `p a`
    // matching expected, edit_distance came out 0 — but the audio itself
    // is unusable). Cleanest fix: don't trust verification on audio Google
    // already flagged as defective; surface the TTS warning as the failure.
    if let Some(note) = tts_note {
        return Ok(ClipVerification {
            actor: actor.to_string(),
            text: text.to_string(),
            wav_path: format!("google-tts://{}", voice.voice_name),
            predicted_raw: Vec::new(),
            predicted_normalized: Vec::new(),
            expected: None,
            variants_considered: 0,
            edit_distance: None,
            edit_distance_pct: None,
            alignment: None,
            failure_reason: Some(note),
        });
    }

    // Google TTS clips don't have per-clip transcription overrides — they're
    // synthesized from the canonical text, so the wikipron+espeak ground
    // truth is the right reference.
    let expected = expected_phoneme_variants(ctx, text, None);
    verify_clip_bytes(
        ctx,
        actor,
        text,
        &format!("google-tts://{}", voice.voice_name),
        &audio_bytes,
        expected,
    )
    .await
}

use std::sync::atomic::{AtomicBool, Ordering};

static CACHE_ONLY: AtomicBool = AtomicBool::new(false);

/// Refuse every network call process-wide: a phoneme prediction or TTS
/// synthesis that isn't already cached becomes an error. generate-data
/// mirrors its own `--cache-only` flag here.
pub fn set_cache_only(enabled: bool) {
    CACHE_ONLY.store(enabled, Ordering::Relaxed);
}

pub fn cache_only() -> bool {
    CACHE_ONLY.load(Ordering::Relaxed)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_suprasegmentals() {
        let lang = Language::English;
        assert_eq!(normalize_phoneme("ˈa", lang), Some("a".to_string()));
        assert_eq!(normalize_phoneme("ˌb", lang), Some("b".to_string()));
        assert_eq!(normalize_phoneme("iː", lang), Some("i".to_string()));
        assert_eq!(normalize_phoneme(".", lang), None);
        assert_eq!(normalize_phoneme("ˈ", lang), None);
        // Combining diacritics inside the phoneme are preserved.
        assert_eq!(normalize_phoneme("ã", lang), Some("ã".to_string()));
    }

    #[test]
    fn normalize_strips_tone_digits_and_liaison() {
        let lang = Language::French;
        // Mandarin tone digits leak through the multilingual wav2vec2 model.
        assert_eq!(normalize_phoneme("y5", lang), Some("y".to_string()));
        assert_eq!(normalize_phoneme("i5", lang), Some("i".to_string()));
        assert_eq!(normalize_phoneme("a5", lang), Some("a".to_string()));
        // Liaison marker has no phonetic content.
        assert_eq!(normalize_phoneme("‿", lang), None);
        assert_eq!(normalize_phoneme("a‿", lang), Some("a".to_string()));
    }

    #[test]
    fn french_canonicalization() {
        let lang = Language::French;
        assert_eq!(normalize_phoneme("r", lang), Some("ʁ".to_string()));
        assert_eq!(normalize_phoneme("ʁ", lang), Some("ʁ".to_string()));
        assert_eq!(normalize_phoneme("ts", lang), Some("t".to_string()));
        assert_eq!(normalize_phoneme("tɕ", lang), Some("t".to_string()));
        assert_eq!(normalize_phoneme("ɥ", lang), Some("y".to_string()));
        // Non-French langs: no canonicalization, just pass-through.
        assert_eq!(
            normalize_phoneme("r", Language::English),
            Some("r".to_string())
        );
    }

    fn ap(main: &str, others: &[&str]) -> language_utils::Pronunciations {
        language_utils::Pronunciations {
            main: main.to_string(),
            others: others.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn ground_truth_concatenates_per_word_single_variant() {
        let mut wp = HashMap::new();
        wp.insert("bonjour".to_string(), ap("b ɔ̃ ʒ u ʁ", &[]));
        wp.insert("madame".to_string(), ap("m a d a m", &[]));
        // Use a language without espeak support (Korean is disabled) so
        // the test isolates the wikipron-only path; espeak coverage is
        // covered by the `espeak::tests` module.
        let variants =
            ground_truth_phoneme_variants("Bonjour, madame!", &wp, Language::Korean).unwrap();
        assert_eq!(variants.len(), 1);
        assert_eq!(
            variants[0],
            vec!["b", "ɔ̃", "ʒ", "u", "ʁ", "m", "a", "d", "a", "m"]
        );
    }

    #[test]
    fn ground_truth_enumerates_per_word_variants() {
        // `mes` has both /me/ and /mɛ/ in wikipron — phrase "mes" alone
        // should produce both candidate sequences. Use Korean to skip
        // the espeak addition (which would inject a third candidate).
        let mut wp = HashMap::new();
        wp.insert("mes".to_string(), ap("m e", &["m ɛ"]));
        let variants = ground_truth_phoneme_variants("mes", &wp, Language::Korean).unwrap();
        assert_eq!(variants.len(), 2);
        assert!(variants.contains(&vec!["m".to_string(), "e".to_string()]));
        assert!(variants.contains(&vec!["m".to_string(), "ɛ".to_string()]));
    }

    #[test]
    fn ground_truth_cross_product_across_words() {
        let mut wp = HashMap::new();
        wp.insert("mes".to_string(), ap("m e", &["m ɛ"]));
        wp.insert("amis".to_string(), ap("a m i", &["a m i z"]));
        let variants = ground_truth_phoneme_variants("mes amis", &wp, Language::Korean).unwrap();
        // 2 × 2 = 4 phrase candidates
        assert_eq!(variants.len(), 4);
    }

    #[test]
    fn ground_truth_returns_none_on_missing_word() {
        let mut wp = HashMap::new();
        wp.insert("bonjour".to_string(), ap("b ɔ̃ ʒ u ʁ", &[]));
        assert!(ground_truth_phoneme_variants("bonjour madame", &wp, Language::Korean).is_none());
    }

    #[test]
    fn ground_truth_falls_back_to_main_when_too_many_combinations() {
        // 5 words × 2 variants each = 32 > MAX_VARIANT_COMBINATIONS (16) →
        // fall back to single main-only candidate (no espeak under Korean).
        let mut wp = HashMap::new();
        for w in &["a", "b", "c", "d", "e"] {
            wp.insert(w.to_string(), ap("X", &["Y"]));
        }
        let variants = ground_truth_phoneme_variants("a b c d e", &wp, Language::Korean).unwrap();
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0], vec!["X"; 5]);
    }

    // Ignored in CI: requires the espeak-ng binary (and the liaison output
    // depends on our custom French-stress-liaison build). Run locally with
    // ESPEAK_NG_BIN set: `cargo test -p generate-data -- --ignored`.
    #[test]
    #[ignore = "requires espeak-ng binary (custom French-liaison build)"]
    fn ground_truth_includes_espeak_variant_for_supported_languages() {
        // For French (espeak-supported), the candidate list should include
        // an espeak-derived variant in addition to the wikipron cross-product.
        // `on est` is the motivating case — espeak produces /ɔ̃ n ɛ/
        // (nasal vowel preserved + liaison /n/), which neither of the
        // wikipron per-word combinations captures.
        let mut wp = HashMap::new();
        wp.insert("on".to_string(), ap("ɔ̃", &["ɔ . n ‿"]));
        wp.insert("est".to_string(), ap("ɛ", &["e"]));
        let variants = ground_truth_phoneme_variants("on est", &wp, Language::French).unwrap();
        // Must include the espeak liaison candidate that the per-word
        // cross-product can't reach.
        assert!(
            variants.contains(&vec!["ɔ̃".to_string(), "n".to_string(), "ɛ".to_string()]),
            "expected espeak liaison candidate /ɔ̃ n ɛ/ in candidates: {variants:?}"
        );
    }

    #[test]
    fn pad_to_min_length_pads_symmetrically() {
        let s = vec![1.0_f32, 2.0, 3.0];
        // 5 needed: 1 lead, 1 trail (3 + 2 = 5)
        let padded = pad_to_min_length(s, 5);
        assert_eq!(padded, vec![0.0, 1.0, 2.0, 3.0, 0.0]);
        // No-op when already at length.
        let s = vec![1.0_f32, 2.0, 3.0];
        let padded = pad_to_min_length(s.clone(), 3);
        assert_eq!(padded, s);
        // Odd needed: extra sample goes on the trailing side.
        let s = vec![1.0_f32];
        let padded = pad_to_min_length(s, 4);
        assert_eq!(padded, vec![0.0, 1.0, 0.0, 0.0]);
    }

    /// Build a degenerate top-k where each predicted phoneme is the only
    /// alternative with probability 1.0. Lets the older alignment tests
    /// keep using the simple `align(predicted, expected)` shape.
    fn fake_topk(predicted: &[String]) -> Vec<Vec<(String, f64)>> {
        predicted.iter().map(|p| vec![(p.clone(), 1.0)]).collect()
    }

    #[test]
    fn align_basics() {
        let s = |slice: &[&str]| slice.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let cases: &[(&[&str], &[&str], usize)] = &[
            (&["a", "b", "c"], &["a", "b", "c"], 0),
            (&["a", "b", "c"], &["a", "x", "c"], 1),
            (&["a", "b"], &["a", "b", "c"], 1),
            (&[], &["a", "b"], 2),
        ];
        for (predicted, expected, want_cost) in cases {
            let predicted_v = s(predicted);
            let expected_v = s(expected);
            let topk = fake_topk(&predicted_v);
            assert_eq!(align(&predicted_v, &topk, &expected_v).0, *want_cost);
        }
    }

    #[test]
    fn align_produces_readable_ops() {
        let s = |slice: &[&str]| slice.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // predicted: a b c d e
        // expected:  a x c   e
        // Optimal: match a, sub b→x, match c, extra d, match e  (cost 2)
        let predicted = s(&["a", "b", "c", "d", "e"]);
        let topk = fake_topk(&predicted);
        let (cost, ops) = align(&predicted, &topk, &s(&["a", "x", "c", "e"]));
        assert_eq!(cost, 2);
        let mut reconstructed_predicted = Vec::new();
        let mut reconstructed_expected = Vec::new();
        for op in &ops {
            match op {
                AlignmentOp::Match { phoneme, .. } => {
                    reconstructed_predicted.push(phoneme.clone());
                    reconstructed_expected.push(phoneme.clone());
                }
                AlignmentOp::Sub {
                    expected,
                    predicted,
                    ..
                } => {
                    reconstructed_predicted.push(predicted.clone());
                    reconstructed_expected.push(expected.clone());
                }
                AlignmentOp::Extra { predicted, .. } => {
                    reconstructed_predicted.push(predicted.clone());
                }
                AlignmentOp::Missing { expected } => {
                    reconstructed_expected.push(expected.clone());
                }
            }
        }
        assert_eq!(reconstructed_predicted, vec!["a", "b", "c", "d", "e"]);
        assert_eq!(reconstructed_expected, vec!["a", "x", "c", "e"]);
    }

    #[test]
    fn sub_carries_probabilities() {
        // Predicted "i" with prob 0.6 at position 0; "e" was the model's
        // runner-up at prob 0.3. Expected is "e", so the Sub op should
        // report predicted_prob = 0.6 and expected_prob = Some(0.3).
        let predicted = vec!["i".to_string()];
        let topk = vec![vec![("i".to_string(), 0.6), ("e".to_string(), 0.3)]];
        let expected = vec!["e".to_string()];
        let (_, ops) = align(&predicted, &topk, &expected);
        match &ops[0] {
            AlignmentOp::Sub {
                predicted_prob,
                expected_prob,
                ..
            } => {
                assert!((predicted_prob - 0.6).abs() < 1e-9);
                assert_eq!(*expected_prob, Some(0.3));
            }
            other => panic!("expected Sub, got {other:?}"),
        }
    }

    #[test]
    fn sub_reports_none_when_expected_not_in_topk() {
        let predicted = vec!["i".to_string()];
        let topk = vec![vec![("i".to_string(), 0.95)]]; // only one alt
        let expected = vec!["œ".to_string()];
        let (_, ops) = align(&predicted, &topk, &expected);
        match &ops[0] {
            AlignmentOp::Sub { expected_prob, .. } => {
                assert_eq!(*expected_prob, None);
            }
            other => panic!("expected Sub, got {other:?}"),
        }
    }

    #[test]
    fn normalize_with_topk_merges_canonical_equivalents() {
        // Raw top-k has both `r` and `ʁ`; canonicalization collapses them
        // to `ʁ`, and the merged top-k should have one `ʁ` entry with the
        // summed probability.
        let raw_phonemes = vec!["r".to_string()];
        let raw_topk = vec![vec![
            RawPhonemeAlt {
                phoneme: "r".to_string(),
                probability: 0.4,
            },
            RawPhonemeAlt {
                phoneme: "ʁ".to_string(),
                probability: 0.3,
            },
            RawPhonemeAlt {
                phoneme: "a".to_string(),
                probability: 0.2,
            },
        ]];
        let (norm, norm_topk) = normalize_with_topk(&raw_phonemes, &raw_topk, Language::French);
        assert_eq!(norm, vec!["ʁ".to_string()]);
        assert_eq!(norm_topk[0][0].0, "ʁ");
        assert!((norm_topk[0][0].1 - 0.7).abs() < 1e-9);
        assert_eq!(norm_topk[0][1].0, "a");
    }
}
