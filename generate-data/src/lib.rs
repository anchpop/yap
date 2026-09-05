#[cfg(test)]
mod db_info;

use std::sync::atomic::{AtomicBool, Ordering};

static CACHE_ONLY: AtomicBool = AtomicBool::new(false);

/// Enable cache-only mode process-wide. In this mode, tysm ChatClients,
/// the Translator, and lexide tokenization will never make network calls —
/// cache misses produce errors (or are skipped, for lexide).
pub fn set_cache_only(enabled: bool) {
    CACHE_ONLY.store(enabled, Ordering::Relaxed);
    phoneme_verify::set_cache_only(enabled);
}

pub fn cache_only() -> bool {
    CACHE_ONLY.load(Ordering::Relaxed)
}

/// Update an indicatif bar from a Batch API status poll. `offset` is the number
/// of items handled by earlier batches and `expected` includes cache hits, which
/// OpenAI's request counts do not include.
pub fn report_batch_progress(
    progress: &indicatif::ProgressBar,
    offset: u64,
    expected: usize,
    batch: &tysm::batch::Batch,
) {
    let total = u64::from(batch.request_counts.total);
    let processed = u64::from(batch.request_counts.completed + batch.request_counts.failed);
    let cached = (expected as u64).saturating_sub(total);
    let position = offset + cached + processed;
    progress.set_position(position.min(progress.length().unwrap_or(u64::MAX)));
}

/// Apply the process-wide cache-only setting to a tysm ChatClient.
pub fn apply_cache_only(
    client: tysm::chat_completions::ChatClient,
) -> tysm::chat_completions::ChatClient {
    if cache_only() {
        client.with_cached_only()
    } else {
        client
    }
}

fn cached_chat_client(model: &str, reasoning_effort: &str) -> tysm::chat_completions::ChatClient {
    base_chat_client(model)
        .with_reasoning_effort(reasoning_effort)
        .with_service_tier("flex")
}

/// Wall-clock stage timer for profiling pipeline runs: each `lap` logs the
/// time since the previous one at info level (`RUST_LOG=generate_data=info`).
pub struct StageTimer {
    started: std::time::Instant,
    last: std::time::Instant,
}

impl StageTimer {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            started: now,
            last: now,
        }
    }

    pub fn lap(&mut self, label: &str) {
        let now = std::time::Instant::now();
        log::info!(
            "timing[{label}]: {:.1}s (total {:.1}s)",
            (now - self.last).as_secs_f64(),
            (now - self.started).as_secs_f64()
        );
        self.last = now;
    }
}

/// Every chat client in this crate starts here, so the Batch API escape hatch
/// below needs to exist in exactly one place.
fn base_chat_client(model: &str) -> tysm::chat_completions::ChatClient {
    let client = tysm::chat_completions::ChatClient::from_env(model)
        .unwrap()
        .with_cache_directory("./.cache");
    if movie_subtitles::llm_segment::no_batch() {
        // Every batch is "small", so tysm sends its cache misses live.
        client.with_small_batch_threshold(usize::MAX)
    } else {
        client
    }
}

/// A current generation client whose cache is checked first, followed by historical model
/// configurations newest-to-oldest. Only the current model may make an API request.
pub fn migrating_chat_client(model: &str) -> tysm::chat_completions::ChatClient {
    apply_cache_only(
        cached_chat_client(model, "low")
            .with_cache_fallback(cached_chat_client("gpt-5.4", "high"))
            .with_cache_fallback(cached_chat_client("gpt-5.4", "low"))
            .with_cache_fallback(cached_chat_client("gpt-5.4-mini", "low"))
            .with_cache_fallback(base_chat_client("gpt-5.4-nano"))
            .with_cache_fallback(base_chat_client("gpt-5.2").with_reasoning_effort("high"))
            .with_cache_fallback(cached_chat_client("gpt-5.2", "low"))
            .with_cache_fallback(base_chat_client("gpt-5"))
            .with_cache_fallback(base_chat_client("gpt-5").with_service_tier("flex"))
            .with_cache_fallback(base_chat_client("gpt-4o")),
    )
}

pub use phoneme_verify as audio_verification;
pub mod books;
pub mod cache_remote;
pub mod dict;
pub mod disambiguation_practice;
pub mod etymology;
pub mod frequencies;
pub mod gold;
pub mod human_audio;
pub mod lexide_token;
pub mod llm_etymology;
pub mod morpheme_info;
pub mod morphology_analysis;
pub mod nlp;
pub mod pipeline;
pub mod pronunciation_patterns;
pub mod pronunciations;
pub mod proper_noun_definitions;
pub mod read_anki;
pub mod slot_analysis;
pub mod target_sentences;
pub mod tatoeba;
pub mod token_embeddings;
pub mod tokenize;
pub mod translate;
pub mod usage_discovery;
pub mod wiktionary_conjugations;
pub mod wiktionary_terms;
