use rustc_hash::{FxHashMap, FxHashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
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

    // Get language codes as bytes for fast comparison
    let target_lang_code = course.target_language.iso_639_3();
    let native_lang_code = course.native_language.iso_639_3();

    // First pass: read sentences, only storing those in our target or native language.
    // Use a reusable line buffer to avoid per-line String allocation.
    let mut sentences_by_id: FxHashMap<u64, (bool, String)> = FxHashMap::default(); // (is_target, text)
    let mut target_sentence_ids: Vec<u64> = Vec::new();

    let file = match File::open(&sentences_file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open sentences file: {e}");
            return vec![];
        }
    };

    let mut reader = BufReader::with_capacity(1 << 16, file);
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        match reader.read_line(&mut line_buf) {
            Ok(0) => break,
            Err(_) => continue,
            Ok(_) => {}
        }

        let line = line_buf.trim_end_matches('\n').trim_end_matches('\r');

        // Skip BOM if present
        let line = line.strip_prefix('\u{feff}').unwrap_or(line);

        // Quick parse: find tabs to extract fields without collecting into Vec
        let Some(tab1) = line.find('\t') else {
            continue;
        };
        let rest = &line[tab1 + 1..];
        let Some(tab2) = rest.find('\t') else {
            continue;
        };

        let id_str = &line[..tab1];
        let lang = rest[..tab2].trim();
        let text = rest[tab2 + 1..].trim();

        // Only process sentences in our target or native language
        if lang != target_lang_code && lang != native_lang_code {
            continue;
        }

        let id = match id_str.parse::<u64>() {
            Ok(id) => id,
            Err(_) => continue,
        };

        let is_target = lang == target_lang_code;
        if is_target {
            target_sentence_ids.push(id);
        }
        sentences_by_id.insert(id, (is_target, text.to_string()));
    }

    // Collect known sentence IDs into a set for fast lookup when filtering links
    let known_ids: FxHashSet<u64> = sentences_by_id.keys().copied().collect();

    // Second pass: read links, only storing those where id1 is a sentence we know about.
    // This filters ~27M links down to only the relevant ones.
    let file = match File::open(&links_file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open links file: {e}");
            return vec![];
        }
    };

    let mut links_map: FxHashMap<u64, Vec<u64>> = FxHashMap::default();
    let mut reader = BufReader::with_capacity(1 << 16, file);

    loop {
        line_buf.clear();
        match reader.read_line(&mut line_buf) {
            Ok(0) => break,
            Err(_) => continue,
            Ok(_) => {}
        }

        let line = line_buf.trim_end_matches('\n').trim_end_matches('\r');

        // Skip BOM if present
        let line = line.strip_prefix('\u{feff}').unwrap_or(line);

        // Quick parse without collecting into Vec
        let Some(tab) = line.find('\t') else {
            continue;
        };

        let id1 = match line[..tab].parse::<u64>() {
            Ok(id) => id,
            Err(_) => continue,
        };

        // Only store links from sentences we care about
        if !known_ids.contains(&id1) {
            continue;
        }

        let id2 = match line[tab + 1..].trim().parse::<u64>() {
            Ok(id) => id,
            Err(_) => continue,
        };

        links_map.entry(id1).or_default().push(id2);
    }

    // Third pass: create pairs from target sentences that have native translations
    let mut pairs = Vec::new();

    for target_id in target_sentence_ids {
        let target_text = match sentences_by_id.get(&target_id) {
            Some((true, text)) => text,
            _ => continue,
        };

        // Find linked sentences
        let linked_ids = match links_map.get(&target_id) {
            Some(ids) => ids,
            None => continue,
        };

        // Look for a native language translation
        for linked_id in linked_ids {
            if let Some((false, native_text)) = sentences_by_id.get(linked_id) {
                // Apply filtering criteria
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
    let mut seen_targets = std::collections::HashSet::new();
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
