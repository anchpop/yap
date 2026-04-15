#[cfg(test)]
mod db_info;

use std::sync::atomic::{AtomicBool, Ordering};

static CACHE_ONLY: AtomicBool = AtomicBool::new(false);

/// Enable cache-only mode process-wide. In this mode, tysm ChatClients,
/// GoogleTranslator, and lexide tokenization will never make network calls —
/// cache misses produce errors (or are skipped, for lexide).
pub fn set_cache_only(enabled: bool) {
    CACHE_ONLY.store(enabled, Ordering::Relaxed);
}

pub fn cache_only() -> bool {
    CACHE_ONLY.load(Ordering::Relaxed)
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

pub mod dict;
pub mod disambiguation_practice;
pub mod frequencies;
pub mod lexide_token;
pub mod morphology;
pub mod morphology_analysis;
pub mod nlp;
pub mod pronunciation_patterns;
pub mod pronunciations;
pub mod proper_noun_definitions;
pub mod read_anki;
pub mod target_sentences;
pub mod tatoeba;
pub mod tokenize;
pub mod wiktionary_conjugations;
pub mod wiktionary_terms;
