//! Per-token contextual embeddings for the sentence database, via the
//! `token-embeddings` Modal endpoint (`modal-envs/token_embeddings.py`).
//!
//! For every sentence in a course's final NLP sentence set, we embed each
//! heteronym token (subword-mean-pooled hidden state from one fixed layer of a
//! multilingual bidirectional encoder) and store the vectors in the osmo cache
//! store. Nothing downstream consumes them yet — they are the substrate for
//! sense discrimination (splitting e.g. "a tear in the paper" from "he shed a
//! tear" into different atoms), where model/layer were chosen by the probe
//! sweeps in `experiments/polysemy/`.
//!
//! Cache layout: `token-embed/{version}/{lang}/{xxh3(text):016x}` → binary
//! record (see [`encode_record`]): header of `dim`, `n`, the `n` word indices
//! (into `SentenceInfo::words`) that were embedded, then `n * dim` f16 values.
//! Keys are per target language, so a sentence shared by several courses is
//! embedded once.

use anyhow::{Context, Result};
use base64::Engine;
use futures::StreamExt;
use language_utils::{Language, SentenceInfo, WordType};
use serde::Deserialize;
use std::sync::LazyLock;
use xxhash_rust::xxh3::xxh3_64;

static MODAL_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("TOKEN_EMBED_ENDPOINT_URL").unwrap_or_else(|_| {
        "https://anchpop--token-embeddings-tokenembedder-embed.modal.run".to_string()
    })
});

/// Cache partition. Bump whenever the model, revision, layer, pooling, or
/// record format changes; must stay in sync with the pins in
/// `modal-envs/token_embeddings.py` (which the deploy marker check enforces).
const CACHE_VERSION: &str = "bge-m3@5617a9f61b02__L17_v1";

/// The deploy marker the endpoint must report (`{revision[:12]}@L{layer}`).
/// A mismatch means the endpoint serves a different model/layer than this
/// cache partition records, so we refuse to write.
const EXPECTED_DEPLOY_MARKER: &str = "5617a9f61b02@L17";

/// Sentences per HTTP request. Short subtitle-register sentences: 96 keeps the
/// request bodies and GPU batches comfortable.
const SENTENCES_PER_REQUEST: usize = 96;

/// Concurrent in-flight requests. The endpoint autoscales up to as many
/// containers (`max_containers` in token_embeddings.py), so each in-flight
/// request gets its own GPU; total GPU-seconds billed are the same either way.
const CONCURRENT_REQUESTS: usize = 10;

const MAX_ATTEMPTS: usize = 5;

fn is_transient_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

fn cache_key(language: Language, sentence: &str) -> String {
    let hash = xxh3_64(sentence.as_bytes());
    format!(
        "token-embed/{CACHE_VERSION}/{}/{hash:016x}",
        language.code()
    )
}

/// The char spans of the tokens we embed: every heteronym word (words with a
/// lemma + POS — the tokens that can become sense-split atoms). Returns
/// `(word_index, start, end)` in char offsets of the sentence text as
/// reconstructed from the words' text + whitespace.
fn heteronym_spans(info: &SentenceInfo) -> Vec<(u32, u32, u32)> {
    let mut spans = Vec::new();
    let mut offset = 0u32;
    for (i, literal) in info.words.iter().enumerate() {
        let len = literal.word.text.chars().count() as u32;
        if matches!(literal.word.word_type, WordType::Heteronym(_)) && len > 0 {
            spans.push((i as u32, offset, offset + len));
        }
        offset += len + literal.whitespace.chars().count() as u32;
    }
    spans
}

/// The sentence text the spans index into. Built from the same words the spans
/// were computed from, so offsets always agree (unlike the map key, which may
/// differ in capitalization).
fn sentence_text(info: &SentenceInfo) -> String {
    let mut text = String::new();
    for literal in &info.words {
        text.push_str(&literal.word.text);
        text.push_str(&literal.whitespace);
    }
    text
}

/// `dim` (u32 LE), `n` (u32 LE), `n` word indices (u32 LE each), then
/// `n * dim` f16 LE values as returned by the endpoint.
fn encode_record(dim: u32, word_indices: &[(u32, u32, u32)], f16_bytes: &[u8]) -> Vec<u8> {
    let n = word_indices.len() as u32;
    let mut out = Vec::with_capacity(8 + word_indices.len() * 4 + f16_bytes.len());
    out.extend_from_slice(&dim.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    for (i, _, _) in word_indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out.extend_from_slice(f16_bytes);
    out
}

/// One sentence prepared for embedding: (cache key, text, heteronym spans).
type SentenceBatchItem = (String, String, Vec<(u32, u32, u32)>);

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    dim: u32,
    vectors: Vec<String>,
    deploy_marker: String,
}

/// Ensure every sentence in `sentences` has its token embeddings in the cache
/// store. Reads are cheap; only cache misses go to the Modal endpoint.
pub async fn ensure_token_embeddings(
    language: Language,
    sentences: &[(String, SentenceInfo)],
    store: &osmo::Store,
    http: &reqwest::Client,
) -> Result<()> {
    // (key, text, spans) for sentences with at least one heteronym token.
    let mut candidates = Vec::new();
    for (sentence, info) in sentences {
        let spans = heteronym_spans(info);
        if spans.is_empty() {
            continue;
        }
        candidates.push((cache_key(language, sentence), sentence_text(info), spans));
    }

    let mut misses = Vec::new();
    for candidate in candidates {
        if store.read(&candidate.0).await.is_none() {
            misses.push(candidate);
        }
    }
    if misses.is_empty() {
        println!(
            "token-embed[{}]: all {} sentences already cached",
            language.code(),
            sentences.len()
        );
        return Ok(());
    }
    if crate::cache_only() {
        log::warn!(
            "token-embed[{}]: {} cache misses skipped (cache-only mode)",
            language.code(),
            misses.len()
        );
        return Ok(());
    }
    println!(
        "token-embed[{}]: embedding {} uncached sentences…",
        language.code(),
        misses.len()
    );

    let total_batches = misses.len().div_ceil(SENTENCES_PER_REQUEST);
    let results: Vec<Result<usize>> =
        futures::stream::iter(misses.chunks(SENTENCES_PER_REQUEST).enumerate().map(
            |(batch_idx, batch)| async move {
                let written = embed_batch(store, http, batch).await?;
                if (batch_idx + 1) % 20 == 0 || batch_idx + 1 == total_batches {
                    println!(
                        "token-embed[{}]: batch {}/{total_batches}",
                        language.code(),
                        batch_idx + 1
                    );
                }
                Ok(written)
            },
        ))
        .buffer_unordered(CONCURRENT_REQUESTS)
        .collect()
        .await;

    let mut written = 0;
    for r in results {
        written += r?;
    }
    println!(
        "token-embed[{}]: wrote {written} sentence embedding records",
        language.code()
    );
    Ok(())
}

/// Embed one batch and write each sentence's record to the store. Returns how
/// many records were written.
async fn embed_batch(
    store: &osmo::Store,
    http: &reqwest::Client,
    batch: &[SentenceBatchItem],
) -> Result<usize> {
    let payload = serde_json::json!({
        "sentences": batch
            .iter()
            .map(|(_, text, spans)| {
                serde_json::json!({
                    "text": text,
                    "spans": spans.iter().map(|(_, a, b)| [a, b]).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    });

    // Retry transient endpoint failures (cold-start 408s, rate limits, 5xx)
    // with linear backoff, same as the wav2vec2 caller.
    let response: EmbedResponse = {
        let mut last_err: Option<anyhow::Error> = None;
        let mut got: Option<EmbedResponse> = None;
        for attempt in 1..=MAX_ATTEMPTS {
            match http.post(MODAL_URL.as_str()).json(&payload).send().await {
                Err(e) => {
                    last_err = Some(anyhow::Error::new(e).context("Modal request transport error"));
                }
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<EmbedResponse>().await {
                            Ok(m) => {
                                got = Some(m);
                                break;
                            }
                            Err(e) => {
                                last_err = Some(
                                    anyhow::Error::new(e)
                                        .context("Failed to parse token-embeddings response"),
                                );
                            }
                        }
                    } else if is_transient_status(status) {
                        let body = response.text().await.unwrap_or_default();
                        last_err = Some(anyhow::anyhow!(
                            "token-embeddings transient {status}: {body}"
                        ));
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        anyhow::bail!("token-embeddings error ({status}): {body}");
                    }
                }
            }
            if attempt < MAX_ATTEMPTS {
                let delay = std::time::Duration::from_secs(5 * attempt as u64);
                log::warn!(
                    "token-embeddings call failed (attempt {attempt}/{MAX_ATTEMPTS}), retrying in {}s",
                    delay.as_secs()
                );
                tokio::time::sleep(delay).await;
            }
        }
        got.ok_or_else(|| {
            last_err
                .unwrap_or_else(|| anyhow::anyhow!("token-embeddings call failed"))
                .context(format!(
                    "token-embeddings endpoint failed after {MAX_ATTEMPTS} attempts"
                ))
        })?
    };

    // Per-response freshness check: refuse to cache vectors served by a
    // container running a different model/layer than this cache partition.
    anyhow::ensure!(
        response.deploy_marker == EXPECTED_DEPLOY_MARKER,
        "deploy-marker mismatch: endpoint reported {:?}, expected {EXPECTED_DEPLOY_MARKER:?} — \
         refusing to cache embeddings from an unexpected model/layer",
        response.deploy_marker
    );
    anyhow::ensure!(
        response.vectors.len() == batch.len(),
        "token-embeddings returned {} vectors for {} sentences",
        response.vectors.len(),
        batch.len()
    );

    let mut written = 0;
    for ((key, _, spans), vec_b64) in batch.iter().zip(&response.vectors) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(vec_b64)
            .context("Failed to decode base64 embedding")?;
        let expected_len = spans.len() * response.dim as usize * 2;
        anyhow::ensure!(
            bytes.len() == expected_len,
            "embedding record length mismatch: got {} bytes, expected {expected_len} \
             ({} spans × {} dims × 2 bytes)",
            bytes.len(),
            spans.len(),
            response.dim
        );
        store
            .write(key, &encode_record(response.dim, spans, &bytes))
            .await
            .context("Failed to persist embedding record")?;
        written += 1;
    }
    Ok(written)
}
