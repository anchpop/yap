#[cfg(test)]
mod db_info;

use std::sync::atomic::{AtomicBool, Ordering};

static CACHE_ONLY: AtomicBool = AtomicBool::new(false);

/// Enable cache-only mode process-wide. In this mode, tysm ChatClients,
/// the Translator, and lexide tokenization will never make network calls —
/// cache misses produce errors (or are skipped, for lexide).
pub fn set_cache_only(enabled: bool) {
    CACHE_ONLY.store(enabled, Ordering::Relaxed);
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
    tysm::chat_completions::ChatClient::from_env(model)
        .unwrap()
        .with_cache_directory("./.cache")
        .with_reasoning_effort(reasoning_effort)
        .with_service_tier("flex")
}

fn cached_default_chat_client(model: &str) -> tysm::chat_completions::ChatClient {
    tysm::chat_completions::ChatClient::from_env(model)
        .unwrap()
        .with_cache_directory("./.cache")
}

fn cached_reasoning_chat_client(
    model: &str,
    reasoning_effort: &str,
) -> tysm::chat_completions::ChatClient {
    cached_default_chat_client(model).with_reasoning_effort(reasoning_effort)
}

/// A current generation client whose cache is checked first, followed by historical model
/// configurations newest-to-oldest. Only the current model may make an API request.
pub fn migrating_chat_client(model: &str) -> tysm::chat_completions::ChatClient {
    apply_cache_only(
        cached_chat_client(model, "low")
            .with_cache_fallback(cached_chat_client("gpt-5.4", "high"))
            .with_cache_fallback(cached_chat_client("gpt-5.4", "low"))
            .with_cache_fallback(cached_chat_client("gpt-5.4-mini", "low"))
            .with_cache_fallback(cached_default_chat_client("gpt-5.4-nano"))
            .with_cache_fallback(cached_reasoning_chat_client("gpt-5.2", "high"))
            .with_cache_fallback(cached_chat_client("gpt-5.2", "low"))
            .with_cache_fallback(cached_default_chat_client("gpt-5"))
            .with_cache_fallback(cached_default_chat_client("gpt-5").with_service_tier("flex"))
            .with_cache_fallback(cached_default_chat_client("gpt-4o")),
    )
}

pub mod audio_verification;
pub mod books;
pub mod cache_remote;
pub mod dict;
pub mod disambiguation_practice;
pub mod etymology;
pub mod frequencies;
pub mod human_audio;
pub mod lexide_token;
pub mod llm_etymology;
pub mod morpheme_info;
pub mod morphology_analysis;
pub mod nlp;
pub mod pronunciation_patterns;
pub mod pronunciations;
pub mod proper_noun_definitions;
pub mod read_anki;
pub mod slot_analysis;
pub mod target_sentences;
pub mod tatoeba;
pub mod token_embeddings;
pub mod tokenize;
pub mod wiktionary_conjugations;
pub mod wiktionary_terms;
