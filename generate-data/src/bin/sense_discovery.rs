//! Standalone sense & collocation discovery: builds each language's NLP
//! sentence set with the same pipeline code as generate-data, fills any
//! token-embedding gaps, mines the cached vectors for cluster structure, and
//! writes reviewable proposal files under `generate-data/data/{lang}/`
//! (`sense_candidates.jsonl`, `discovered_multiword_terms.jsonl`) for the
//! next generate-data run to ingest.
//!
//! Usage: cargo run --release --bin sense_discovery -- [--cache-only] [<lang>...]

use anyhow::Context;
use language_utils::COURSES;
use std::collections::BTreeSet;

use generate_data::cache_remote;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenvy::dotenv().ok();
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut lang_filter: BTreeSet<String> = BTreeSet::new();
    let mut cache_only = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--cache-only" => cache_only = true,
            s if s.starts_with("--") => anyhow::bail!("unknown flag: {s}"),
            _ => {
                lang_filter.insert(arg);
            }
        }
    }
    if !lang_filter.is_empty() {
        let known: BTreeSet<&str> = COURSES.iter().map(|c| c.target_language.code()).collect();
        let unknown: Vec<&str> = lang_filter
            .iter()
            .map(String::as_str)
            .filter(|code| !known.contains(code))
            .collect();
        if !unknown.is_empty() {
            anyhow::bail!(
                "unknown language code(s): {}\nknown codes: {}",
                unknown.join(", "),
                known.into_iter().collect::<Vec<_>>().join(", "),
            );
        }
    }
    generate_data::set_cache_only(cache_only);

    cache_remote::warm().await;

    // One run per target language: the sentence construction and the mined
    // occurrences depend only on the target side, so the first course
    // teaching a language stands in for all of them.
    let mut seen = BTreeSet::new();
    for course in COURSES {
        let lang = course.target_language;
        if !lang_filter.is_empty() && !lang_filter.contains(lang.code()) {
            continue;
        }
        if !seen.insert(lang.code().to_string()) {
            continue;
        }
        println!();
        println!("Discovering senses/collocations for {lang}");
        println!("================================================");

        let sentence_corpus = generate_data::target_sentences::get_target_sentences(*course)
            .context("Failed to get target sentences")?;
        let segmented = generate_data::pipeline::segment_corpus(course, &sentence_corpus)
            .await
            .context("Failed to segment corpus")?;
        let store = cache_remote::store();
        generate_data::token_embeddings::ensure_token_embeddings(
            lang,
            segmented.nlp_sentences.iter(),
            &segmented.interners,
            &store,
        )
        .await
        .context("Failed to ensure token embeddings")?;
        generate_data::sense_discovery::discover(lang, &segmented, &store)
            .await
            .context("Sense discovery failed")?;
    }

    cache_remote::flush().await;
    Ok(())
}
