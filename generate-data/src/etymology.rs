//! Etymological segmentation.
//!
//! Words are decomposed into `MorphemeSegment` pairs — surface substring +
//! canonical/lemma form. The segmentations come from two sources:
//!
//!  1. Hand-curated `golden_morphemes.jsonl` (loaded below).
//!  2. LLM-generated splits for words not covered by (1) — see `llm_etymology`.
//!
//! There is no unigram-trainer fallback anymore: the LLM handles every word
//! the golden set doesn't, and its output already has the surface/canonical
//! pair we need.

use language_utils::{Course, MorphemeSegment};
use std::collections::BTreeMap;
use std::path::Path;

/// A word and its surface-aligned morpheme decomposition.
#[derive(Debug, Clone)]
pub struct AlignedEntry {
    pub word: String,
    pub segments: Vec<MorphemeSegment<String>>,
}

/// Build etymology segmentations from a list of aligned entries. Words not
/// present in `aligned` get no entry in the returned map (caller is
/// responsible for providing coverage via the LLM path if needed).
pub fn build_etymology_segmentations(
    aligned: &[AlignedEntry],
    words: &[String],
) -> BTreeMap<String, Vec<MorphemeSegment<String>>> {
    let lookup: std::collections::HashMap<&str, &AlignedEntry> =
        aligned.iter().map(|e| (e.word.as_str(), e)).collect();

    words
        .iter()
        .filter_map(|w| {
            lookup
                .get(w.as_str())
                .map(|entry| (w.clone(), entry.segments.clone()))
        })
        .collect()
}

/// Production wrapper: loads the golden JSONL for the course's target
/// language, fills in missing words via LLM (with per-word morphology context
/// so the model can emit accurate grammatical tags), and runs
/// [`build_etymology_segmentations`].
pub async fn build_etymology_segmentations_with_llm(
    course: Course,
    golden_path: &Path,
    words: &[String],
    word_morphology: &BTreeMap<String, String>,
) -> anyhow::Result<BTreeMap<String, Vec<MorphemeSegment<String>>>> {
    let aligned =
        crate::llm_etymology::augment_with_llm_aligned(course, golden_path, words, word_morphology)
            .await?;
    Ok(build_etymology_segmentations(&aligned, words))
}
