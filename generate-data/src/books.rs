//! Book-derived sentences: translated (or original-language) book prose, pre-segmented
//! into sentences, living at
//! `generate-data/data/<lang>/sentence-sources/books/<series>/<book>.jsonl`
//! (one folder per series; a standalone book is a one-book series). Each series folder
//! also holds a `metadata.jsonl` with one `BookMetadata` line per book.
//!
//! The sentence files are produced by `src/bin/translate_book.rs` (LLM translation of
//! chunks from an English source book, segmented by the parsley `/segment` serve) and are
//! consumed by [`crate::target_sentences::get_target_sentences`] like any other sentence
//! source, so book sentences flow into the gold labeling (clean-nlp-data) and everything
//! downstream.

use std::path::Path;

use language_utils::SentenceSource;
use serde::{Deserialize, Serialize};

/// One sentence extracted from a (possibly translated) book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSentence {
    pub sentence: String,
    /// Book slug, e.g. "pale-lights"
    pub book: String,
    /// Chunk id in the extraction, e.g. "ch012-c03" — ties a sentence back to its context.
    pub chunk_id: String,
}

/// Load all book sentences for a language from `sentence-sources/books/*.jsonl`.
pub fn load_book_sentences(
    source_data_path: &Path,
) -> anyhow::Result<Vec<(String, SentenceSource)>> {
    let books_dir = source_data_path.join("sentence-sources/books");
    if !books_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for series_entry in std::fs::read_dir(&books_dir)? {
        let series_dir = series_entry?.path();
        if !series_dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&series_dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            // metadata.jsonl holds BookMetadata records, not sentences
            if path.file_name().is_some_and(|n| n == "metadata.jsonl") {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                let rec: BookSentence = serde_json::from_str(line)?;
                let sentence = rec.sentence.trim();
                if sentence.is_empty() {
                    continue;
                }
                let mut source = SentenceSource::none();
                source.book_ids = vec![rec.book.clone()];
                out.push((sentence.to_string(), source));
            }
        }
    }
    Ok(out)
}
