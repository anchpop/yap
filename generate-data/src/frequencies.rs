use language_utils::{
    Gram, GramFrequencyEntry, GramVocabEntry, SentenceGram, SentenceGrams, SentenceSource,
};
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Write;

pub fn write_gram_frequencies_file(
    frequencies: &[GramFrequencyEntry<String>],
    output_path: &std::path::Path,
) -> anyhow::Result<()> {
    let mut frequencies = frequencies.to_vec();
    frequencies.sort_by_key(|entry| Reverse(entry.clone()));

    let mut file = File::create(output_path)?;

    for entry in frequencies {
        let json = serde_json::to_string(&entry)?;
        writeln!(file, "{json}")?;
    }

    Ok(())
}

/// Compute per-movie gram frequencies
pub fn compute_movie_gram_frequencies(
    encoded_sentences: &[(String, SentenceGrams<Gram<String>>)],
    sentence_sources: &[(String, SentenceSource)],
    movie_ids: &[String],
    gram_vocabulary: &[GramVocabEntry<String>],
) -> FxHashMap<String, Vec<GramFrequencyEntry<String>>> {
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

    // Build a map from gram to its vocab entry for frequency lookup
    let gram_to_vocab: FxHashMap<&Gram<String>, &GramVocabEntry<String>> = gram_vocabulary
        .iter()
        .map(|entry| (&entry.atoms, entry))
        .collect();

    let mut movie_gram_frequencies = FxHashMap::default();

    println!(
        "Computing per-movie gram frequencies for {} movies...",
        movie_ids.len()
    );

    for movie_id in movie_ids {
        // Count grams in sentences for this movie with weighted contributions
        let mut gram_counts: BTreeMap<Gram<String>, f32> = BTreeMap::new();
        let mut gram_actual_counts: BTreeMap<Gram<String>, u32> = BTreeMap::new();

        for (sentence, sentence_grams) in encoded_sentences {
            // Check if this sentence is from this movie
            if !sentence_to_movies
                .get(sentence.as_str())
                .map(|movie_ids| movie_ids.contains(&movie_id.as_str()))
                .unwrap_or(false)
            {
                continue;
            }

            // Collect the set of learnable grams in the encoded sentence
            let encoded_grams: HashSet<&Gram<String>> = sentence_grams
                .grams
                .iter()
                .filter_map(|g| match g {
                    SentenceGram::Learnable(g) => Some(g),
                    _ => None,
                })
                .collect();

            // Count learnable grams from the encoded sentence (weight 1.0)
            for gram in &encoded_grams {
                *gram_counts.entry((*gram).clone()).or_insert(0.0) += 1.0;
                *gram_actual_counts.entry((*gram).clone()).or_insert(0) += 1;
            }

            // Count high-confidence multiword term grams (weight 0.7),
            // skipping grams already in the encoded sentence
            for gram in &sentence_grams.multiword_terms {
                if !encoded_grams.contains(gram) {
                    *gram_counts.entry(gram.clone()).or_insert(0.0) += 0.7;
                }
            }

            // Count low-confidence multiword term grams (weight 0.3),
            // skipping grams already in the encoded sentence
            for gram in &sentence_grams.low_confidence_multiword_terms {
                if !encoded_grams.contains(gram) {
                    *gram_counts.entry(gram.clone()).or_insert(0.0) += 0.3;
                }
            }
        }

        let mut freq_entries: Vec<GramFrequencyEntry<String>> = Vec::new();

        // Add gram frequencies, rounding up to integers
        for (gram, count) in gram_counts {
            let count = count.ceil() as u32;
            let direct_count = gram_actual_counts.get(&gram).copied().unwrap_or(0);
            // Only include grams that are in the vocabulary and are learnable
            if let Some(vocab_entry) = gram_to_vocab.get(&gram)
                && vocab_entry.atoms.is_learnable()
            {
                freq_entries.push(GramFrequencyEntry {
                    count,
                    direct_count,
                    disambiguation_key: gram.disambiguation_key(),
                    gram,
                });
            }
        }

        if !freq_entries.is_empty() {
            movie_gram_frequencies.insert(movie_id.clone(), freq_entries);
        }
    }

    movie_gram_frequencies
}

/// Compute master gram frequencies from all encoded sentences.
///
/// Weighting: 1.0 × encoded_sentence + 0.7 × high_confidence + 0.3 × low_confidence.
/// Multiword term grams that already appear in the encoded sentence are excluded
/// from the multiword counts to avoid double-counting.
pub fn compute_gram_frequencies(
    encoded_sentences: &[(String, SentenceGrams<Gram<String>>)],
    gram_vocabulary: &[GramVocabEntry<String>],
) -> Vec<GramFrequencyEntry<String>> {
    // Build a map from gram to its vocab entry for frequency lookup
    let gram_to_vocab: FxHashMap<&Gram<String>, &GramVocabEntry<String>> = gram_vocabulary
        .iter()
        .map(|entry| (&entry.atoms, entry))
        .collect();

    // Count grams with weighted contributions, and actual sentence counts separately
    let mut gram_counts: BTreeMap<Gram<String>, f32> = BTreeMap::new();
    let mut gram_actual_counts: BTreeMap<Gram<String>, u32> = BTreeMap::new();

    for (_sentence, sentence_grams) in encoded_sentences {
        // Collect the set of learnable grams in the encoded sentence
        let encoded_grams: HashSet<&Gram<String>> = sentence_grams
            .grams
            .iter()
            .filter_map(|g| match g {
                SentenceGram::Learnable(g) => Some(g),
                _ => None,
            })
            .collect();

        // Count learnable grams from the encoded sentence (weight 1.0)
        for gram in &encoded_grams {
            *gram_counts.entry((*gram).clone()).or_insert(0.0) += 1.0;
            *gram_actual_counts.entry((*gram).clone()).or_insert(0) += 1;
        }

        // Count high-confidence multiword term grams (weight 0.7),
        // skipping grams already in the encoded sentence
        for gram in &sentence_grams.multiword_terms {
            if !encoded_grams.contains(gram) {
                *gram_counts.entry(gram.clone()).or_insert(0.0) += 0.7;
            }
        }

        // Count low-confidence multiword term grams (weight 0.3),
        // skipping grams already in the encoded sentence
        for gram in &sentence_grams.low_confidence_multiword_terms {
            if !encoded_grams.contains(gram) {
                *gram_counts.entry(gram.clone()).or_insert(0.0) += 0.3;
            }
        }
    }

    let mut freq_entries: Vec<GramFrequencyEntry<String>> = Vec::new();

    // Add gram frequencies, rounding up to integers
    for (gram, count) in gram_counts {
        let count = count.ceil() as u32;
        let direct_count = gram_actual_counts.get(&gram).copied().unwrap_or(0);
        // Only include grams that are in the vocabulary and are learnable
        if let Some(vocab_entry) = gram_to_vocab.get(&gram)
            && vocab_entry.atoms.is_learnable()
        {
            freq_entries.push(GramFrequencyEntry {
                count,
                direct_count,
                disambiguation_key: gram.disambiguation_key(),
                gram,
            });
        }
    }

    // Sort by frequency descending
    freq_entries.sort_by_key(|entry| Reverse(entry.clone()));

    freq_entries
}
