use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;

use language_utils::Course;
use sentence_sampler::sample_to_target_with_stats;

pub struct TatoebaPair {
    pub target: String,
    pub native: String,
}

/// Read Tatoeba master dump and extract sentence pairs matching the course languages
///
/// # Arguments
///
/// * `course` - The language course to process
/// * `target_count` - Optional maximum number of sentences to return. If None, uses DEFAULT_TARGET_SENTENCE_COUNT.
///
pub fn get_tatoeba_pairs(
    _data_path: &Path,
    course: Course,
    target_count: usize,
) -> Vec<TatoebaPair> {
    // Use the master Tatoeba dump location
    let tatoeba_dir = Path::new("./generate-data/data/tatoeba");
    let sentences_file = tatoeba_dir.join("sentences.csv");
    let links_file = tatoeba_dir.join("links.csv");

    if !sentences_file.exists() {
        eprintln!(
            "Tatoeba sentences file not found at: {}",
            sentences_file.display()
        );
        return vec![];
    }

    if !links_file.exists() {
        eprintln!("Tatoeba links file not found at: {}", links_file.display());
        return vec![];
    }

    // Get language codes
    let target_lang_code = course.target_language.code();
    let native_lang_code = course.native_language.code();

    // Read entire sentences file at once to avoid per-line allocations
    let sentences_data = match std::fs::read_to_string(&sentences_file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read sentences file: {e}");
            return vec![];
        }
    };

    // Strip BOM once at the start
    let sentences_data = sentences_data
        .strip_prefix('\u{feff}')
        .unwrap_or(&sentences_data);

    // Parse sentences: only store IDs + text for target/native languages
    // Use separate maps for target and native to avoid storing a language tag per entry
    let mut target_texts: FxHashMap<u64, String> = FxHashMap::default();
    let mut native_texts: FxHashMap<u64, String> = FxHashMap::default();
    // Collect all relevant IDs for fast link filtering
    let mut relevant_ids: FxHashSet<u64> = FxHashSet::default();

    for line in sentences_data.lines() {
        // Parse: id [tab] lang [tab] text
        let Some((id_str, rest)) = line.split_once('\t') else {
            continue;
        };
        let Some((lang, text)) = rest.split_once('\t') else {
            continue;
        };

        if lang != target_lang_code && lang != native_lang_code {
            continue;
        }

        let Ok(id) = id_str.parse::<u64>() else {
            continue;
        };

        let text = text.trim();
        relevant_ids.insert(id);
        if lang == target_lang_code {
            target_texts.insert(id, text.to_string());
        } else {
            native_texts.insert(id, text.to_string());
        }
    }

    // Read entire links file at once
    let links_data = match std::fs::read_to_string(&links_file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read links file: {e}");
            return vec![];
        }
    };

    let links_data = links_data.strip_prefix('\u{feff}').unwrap_or(&links_data);

    // Build target_id -> native_ids links (only for relevant pairs)
    let mut links_map: FxHashMap<u64, Vec<u64>> = FxHashMap::default();
    for line in links_data.lines() {
        // Parse: id1 [tab] id2
        let Some((id1_str, id2_str)) = line.split_once('\t') else {
            continue;
        };

        // Fast reject: check first char before parsing
        let Ok(id1) = id1_str.parse::<u64>() else {
            continue;
        };

        // Only care about links FROM a relevant sentence
        if !relevant_ids.contains(&id1) {
            continue;
        }

        let Ok(id2) = id2_str.trim().parse::<u64>() else {
            continue;
        };

        links_map.entry(id1).or_default().push(id2);
    }

    // Drop to free memory before building pairs
    drop(relevant_ids);

    // Create pairs from target sentences that have native translations
    let mut pairs = Vec::new();

    for (&target_id, target_text) in &target_texts {
        let Some(linked_ids) = links_map.get(&target_id) else {
            continue;
        };

        for &linked_id in linked_ids {
            if let Some(native_text) = native_texts.get(&linked_id) {
                if !crate::target_sentences::should_include_pair(target_text, native_text, course) {
                    continue;
                }

                pairs.push(TatoebaPair {
                    target: target_text.clone(),
                    native: native_text.clone(),
                });
                break; // Only take the first native translation
            }
        }
    }

    // Deduplicate based on target sentences
    let mut seen_targets = FxHashSet::default();
    let unique_pairs: Vec<TatoebaPair> = pairs
        .into_iter()
        .filter(|pair| seen_targets.insert(pair.target.clone()))
        .collect();

    // Apply random sampling if we have more sentences than the target
    let (sampled_pairs, _stats) = sample_to_target_with_stats(unique_pairs, target_count, |pair| {
        (pair.target.clone(), pair.native.clone())
    });

    sampled_pairs
}
