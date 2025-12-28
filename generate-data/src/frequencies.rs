use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{FrequencyEntry, Heteronym, Language, Lexeme, SentenceInfo, SentenceSource};
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Write;

/// Compute frequencies for the given sentences.
///
/// Low-confidence multiwords count for 30% of a high-confidence
/// multiword, except for French multiwords that start with "ne",
/// which count fully.
pub fn compute_frequencies(
    sentences: &BTreeMap<String, SentenceInfo>,
    language: Language,
    banned_words: &HashSet<Heteronym<String>>,
) -> BTreeMap<Lexeme<String>, u32> {
    let mut frequencies: BTreeMap<Lexeme<String>, f32> = BTreeMap::new();

    let pb = ProgressBar::new(sentences.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} sentences ({per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    for sentence in sentences.values() {
        pb.inc(1);
        // Count individual words
        for word in &sentence.words {
            if let Some(heteronym) = word.heteronym() {
                if !banned_words.contains(heteronym) {
                    *frequencies
                        .entry(Lexeme::Heteronym(heteronym.clone()))
                        .or_insert(0.0) += 1.0;
                }
            }
        }

        // Count high-confidence multiword terms fully
        for term in &sentence.multiword_terms.high_confidence {
            *frequencies
                .entry(Lexeme::Multiword(term.clone()))
                .or_insert(0.0) += 1.0;
        }

        // Count low-confidence multiword terms with weighting
        for term in &sentence.multiword_terms.low_confidence {
            let weight = if language == Language::French && term.starts_with("ne ") {
                1.0
            } else {
                0.3
            };
            *frequencies
                .entry(Lexeme::Multiword(term.clone()))
                .or_insert(0.0) += weight;
        }
    }

    pb.finish();

    // Round fractional counts to integers for output
    frequencies
        .into_iter()
        .map(|(lexeme, count)| (lexeme, count.ceil() as u32))
        .collect()
}

pub fn write_frequencies_file(
    frequencies: BTreeMap<Lexeme<String>, u32>,
    output_path: &std::path::Path,
) -> anyhow::Result<()> {
    let mut frequencies: Vec<FrequencyEntry<String>> = frequencies
        .into_iter()
        .map(|(lexeme, count)| FrequencyEntry { lexeme, count })
        .collect();

    frequencies.sort_by_key(|entry| Reverse(entry.count));

    let mut file = File::create(output_path)?;

    for entry in frequencies {
        let json = serde_json::to_string(&entry)?;
        writeln!(file, "{json}")?;
    }

    Ok(())
}

/// Compute per-movie word frequencies
pub fn compute_movie_frequencies(
    sentences: &[(String, SentenceInfo)],
    sentence_sources: &[(String, SentenceSource)],
    movie_ids: &[String],
    language: Language,
    banned_words: &HashSet<Heteronym<String>>,
) -> FxHashMap<String, Vec<FrequencyEntry<String>>> {
    // Build a map from sentence to movie IDs
    let sentence_to_movies: FxHashMap<&str, Vec<&str>> = {
        let mut map: FxHashMap<&str, Vec<&str>> = FxHashMap::default();
        for (sentence, source) in sentence_sources {
            if !source.movie_ids.is_empty() {
                map.insert(
                    sentence.as_str(),
                    source.movie_ids.iter().map(|s| s.as_str()).collect(),
                );
            }
        }
        map
    };

    let mut movie_frequencies = FxHashMap::default();

    println!(
        "Computing per-movie frequencies for {} movies...",
        movie_ids.len()
    );

    for movie_id in movie_ids {
        // Filter sentences for this movie
        let movie_sentences: BTreeMap<String, SentenceInfo> = sentences
            .iter()
            .filter(|(sentence, _)| {
                sentence_to_movies
                    .get(sentence.as_str())
                    .map(|movie_ids| movie_ids.contains(&movie_id.as_str()))
                    .unwrap_or(false)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if !movie_sentences.is_empty() {
            let freqs = compute_frequencies(&movie_sentences, language, banned_words);

            let freq_entries: Vec<FrequencyEntry<String>> = freqs
                .into_iter()
                .map(|(lexeme, count)| FrequencyEntry { lexeme, count })
                .collect();

            movie_frequencies.insert(movie_id.clone(), freq_entries);
        }
    }

    movie_frequencies
}
