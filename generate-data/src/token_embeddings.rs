//! Per-token contextual embeddings for the sentence database, via the
//! `token-embeddings` Modal endpoint (`modal-envs/token_embeddings.py`).
//!
//! For every sentence in a course's final NLP sentence set, we embed each
//! heteronym token (subword-mean-pooled hidden state from one fixed layer of a
//! multilingual bidirectional encoder) and store the vectors in the osmo cache
//! store. They are the substrate for sense discrimination (splitting e.g. "a
//! tear in the paper" from "he shed a tear" into different atoms), mined by
//! the `sense_discovery` binary; broad multilingual probe sweeps selected the
//! model and layer.
//!
//! Cache layout: `token-embed/{version}/{lang}/{xxh3(text):016x}` → binary
//! record (see [`encode_record`]): header of `dim`, `n`, the `n` char spans
//! `(start, end)` that were embedded, then `n * dim` f16 values. Records are
//! keyed by spans — not word indices — because the vectors depend only on
//! `(text, span)`: a change to how words are split re-derives which spans are
//! needed but keeps every unchanged span's vector valid. Keys are per target
//! language, so a sentence shared by several courses is embedded once.

use anyhow::{Context, Result};
use base64::Engine;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Language, Literal, SentenceInfo, WordType};
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
const CACHE_VERSION: &str = "bge-m3@5617a9f61b02__L17_v2";

/// The deploy marker the endpoint must report (`{revision[:12]}@L{layer}`).
/// A mismatch means the endpoint serves a different model/layer than this
/// cache partition records, so we refuse to write.
const EXPECTED_DEPLOY_MARKER: &str = "5617a9f61b02@L17";

/// Sentences per HTTP request, matching the endpoint's inner forward-pass
/// batch so each request is one GPU batch. Short subtitle-register sentences
/// keep the request bodies comfortable even at 256.
const SENTENCES_PER_REQUEST: usize = 256;

/// Concurrent in-flight requests. The endpoint autoscales up to as many
/// containers (`max_containers` in token_embeddings.py), so each in-flight
/// request gets its own GPU; total GPU-seconds billed are the same either way.
const CONCURRENT_REQUESTS: usize = 10;

const MAX_ATTEMPTS: usize = 5;

fn is_transient_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

pub fn cache_key(language: Language, sentence: &str) -> String {
    let hash = xxh3_64(sentence.as_bytes());
    format!(
        "token-embed/{CACHE_VERSION}/{}/{hash:016x}",
        language.code()
    )
}

/// The char spans of the tokens we embed: every heteronym word (words with a
/// lemma + POS — the tokens that can become sense-split atoms). Returns
/// `(start, end)` in Unicode code-point offsets of the sentence text as
/// reconstructed from the words' text + whitespace (the convention the HF
/// tokenizer's offset mapping uses).
pub fn heteronym_spans(words: &[Literal<String>]) -> Vec<(u32, u32)> {
    let mut spans = Vec::new();
    let mut offset = 0u32;
    for literal in words {
        let len = literal.word.text.chars().count() as u32;
        if matches!(literal.word.word_type, WordType::Heteronym(_)) && len > 0 {
            spans.push((offset, offset + len));
        }
        offset += len + literal.whitespace.chars().count() as u32;
    }
    spans
}

/// The sentence text the spans index into. Built from the same words the spans
/// were computed from, so offsets always agree (unlike the map key, which may
/// differ in capitalization).
pub fn sentence_text(words: &[Literal<String>]) -> String {
    let mut text = String::new();
    for literal in words {
        text.push_str(&literal.word.text);
        text.push_str(&literal.whitespace);
    }
    text
}

/// `dim` (u32 LE), `n` (u32 LE), `n` spans (`start` u32 LE, `end` u32 LE),
/// then `n * dim` f16 LE values as returned by the endpoint.
fn encode_record(dim: u32, spans: &[(u32, u32)], f16_bytes: &[u8]) -> Vec<u8> {
    let n = spans.len() as u32;
    let mut out = Vec::with_capacity(8 + spans.len() * 8 + f16_bytes.len());
    out.extend_from_slice(&dim.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    for (start, end) in spans {
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&end.to_le_bytes());
    }
    out.extend_from_slice(f16_bytes);
    out
}

/// A decoded record entry: the char span and its pooled vector.
pub type SpanVector = ((u32, u32), Vec<f32>);

/// Decode a cached record into its per-span f32 vectors. `None` for malformed
/// records (so callers treat them as cache misses rather than trusting them).
pub fn decode_record(record: &[u8]) -> Option<Vec<SpanVector>> {
    let header = record.get(..8)?;
    let dim = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
    let n = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
    if record.len() != 8 + n * 8 + n * dim * 2 {
        return None;
    }
    let spans = record[8..8 + n * 8].chunks_exact(8).map(|c| {
        (
            u32::from_le_bytes(c[..4].try_into().unwrap()),
            u32::from_le_bytes(c[4..].try_into().unwrap()),
        )
    });
    let vectors = record[8 + n * 8..].chunks_exact(dim * 2).map(|chunk| {
        chunk
            .chunks_exact(2)
            .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
            .collect::<Vec<f32>>()
    });
    Some(spans.zip(vectors).collect())
}

/// Read the cached vectors for `spans` of a sentence. `None` if the record is
/// missing, malformed, or lacks any requested span.
pub async fn read_word_vectors(
    store: &osmo::Store,
    language: Language,
    sentence_text: &str,
    spans: &[(u32, u32)],
) -> Option<Vec<Vec<f32>>> {
    let record = store.read(&cache_key(language, sentence_text)).await?;
    let decoded = decode_record(&record)?;
    let by_span: std::collections::HashMap<(u32, u32), Vec<f32>> = decoded.into_iter().collect();
    spans
        .iter()
        .map(|span| by_span.get(span).cloned())
        .collect()
}

/// Whether a cached record contains a vector for every span in `needed`.
/// False for malformed records, so they get re-embedded rather than trusted.
fn record_covers(record: &[u8], needed: &[(u32, u32)]) -> bool {
    let Some(decoded) = decode_record(record) else {
        return false;
    };
    let cached: std::collections::HashSet<(u32, u32)> =
        decoded.into_iter().map(|(span, _)| span).collect();
    needed.iter().all(|span| cached.contains(span))
}

/// One sentence prepared for embedding: (cache key, text, heteronym spans).
type SentenceBatchItem = (String, String, Vec<(u32, u32)>);

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    dim: u32,
    vectors: Vec<String>,
    deploy_marker: String,
}

/// Ensure every sentence in `sentences` has its token embeddings in the cache
/// store. Reads are cheap; only cache misses go to the Modal endpoint.
pub async fn ensure_token_embeddings<'a>(
    language: Language,
    sentences: impl IntoIterator<Item = (&'a String, &'a SentenceInfo)>,
    interners: &language_utils::GramInterners,
    store: &osmo::Store,
) -> Result<()> {
    // HTTP/1.1 on purpose: with HTTP/2, reqwest multiplexes every concurrent
    // request over one TCP connection, and hyper's default 64KB flow-control
    // window caps that connection at ~2MB/s — which throttled the ~3.5MB
    // embedding responses to a third of the endpoint's throughput. HTTP/1.1
    // gives each in-flight request its own connection (measured 3x faster).
    let http = &reqwest::Client::builder()
        .http1_only()
        .build()
        .context("Failed to build HTTP client")?;
    // (key, text, spans) for sentences with at least one heteronym token.
    let mut candidates = Vec::new();
    let mut total_sentences = 0usize;
    for (sentence, info) in sentences {
        total_sentences += 1;
        let words = info.decode_words(interners, language);
        let spans = heteronym_spans(&words);
        if spans.is_empty() {
            continue;
        }
        candidates.push((cache_key(language, sentence), sentence_text(&words), spans));
    }

    // A hit must actually cover the spans the current tokenization needs — a
    // record written under a different word-splitting stays valid for its
    // overlapping spans but must be refreshed if any needed span is missing.
    let mut misses = Vec::new();
    for candidate in candidates {
        match store.read(&candidate.0).await {
            Some(record) if record_covers(&record, &candidate.2) => {}
            _ => misses.push(candidate),
        }
    }
    if misses.is_empty() {
        println!(
            "token-embed[{}]: all {total_sentences} sentences already cached",
            language.code(),
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
    let pb = ProgressBar::new(misses.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!(
                "{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {} token embeddings ({{per_sec}}, {{eta}})",
                language.code()
            ))
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let results: Vec<Result<usize>> =
        futures::stream::iter(misses.chunks(SENTENCES_PER_REQUEST).map(|batch| {
            let pb = &pb;
            async move {
                let written = embed_batch(store, http, batch).await?;
                pb.inc(batch.len() as u64);
                Ok(written)
            }
        }))
        .buffer_unordered(CONCURRENT_REQUESTS)
        .collect()
        .await;

    let mut written = 0;
    for r in results {
        written += r?;
    }
    pb.finish_and_clear();
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
                    "spans": spans.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trips() {
        let spans = [(0u32, 4u32), (7, 12)];
        let values = [[1.0f32, -0.5, 0.25], [0.0, 2.0, -1.0]];
        let f16_bytes: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|&v| half::f16::from_f32(v).to_le_bytes())
            .collect();
        let record = encode_record(3, &spans, &f16_bytes);

        let decoded = decode_record(&record).unwrap();
        assert_eq!(decoded.len(), 2);
        for ((span, vector), (expected_span, expected)) in
            decoded.iter().zip(spans.iter().zip(&values))
        {
            assert_eq!(span, expected_span);
            assert_eq!(vector.as_slice(), expected.as_slice());
        }
        assert!(record_covers(&record, &spans));
        assert!(!record_covers(&record, &[(0, 4), (20, 25)]));
        assert!(decode_record(&record[..record.len() - 1]).is_none());
    }
}
