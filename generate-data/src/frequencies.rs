use language_utils::{
    FrequencySourceId, Gram, GramFrequencyEntry, GramVocabEntry, SentenceGram, SentenceGrams,
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

/// Compute per-source gram frequencies.
///
/// `sentence_to_sources` maps each sentence text to the set of source IDs it belongs to.
pub fn compute_per_source_gram_frequencies(
    encoded_sentences: &[(String, SentenceGrams<Gram<String>>)],
    sentence_to_sources: &FxHashMap<String, Vec<FrequencySourceId>>,
    gram_vocabulary: &[GramVocabEntry<String>],
) -> FxHashMap<FrequencySourceId, Vec<GramFrequencyEntry<String>>> {
    // Build a map from gram to its vocab entry for frequency lookup
    let gram_to_vocab: FxHashMap<&Gram<String>, &GramVocabEntry<String>> = gram_vocabulary
        .iter()
        .map(|entry| (&entry.atoms, entry))
        .collect();

    // Collect all unique source IDs
    let all_source_ids: HashSet<&FrequencySourceId> = sentence_to_sources
        .values()
        .flat_map(|ids| ids.iter())
        .collect();

    println!(
        "Computing per-source gram frequencies for {} sources...",
        all_source_ids.len()
    );

    let mut result = FxHashMap::default();

    for source_id in all_source_ids {
        let mut gram_counts: BTreeMap<Gram<String>, f32> = BTreeMap::new();
        let mut gram_actual_counts: BTreeMap<Gram<String>, u32> = BTreeMap::new();

        for (sentence, sentence_grams) in encoded_sentences {
            // Check if this sentence belongs to this source
            let belongs = sentence_to_sources
                .get(sentence)
                .map(|ids| ids.contains(source_id))
                .unwrap_or(false);
            if !belongs {
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
            for term in &sentence_grams.multiword_terms {
                if !encoded_grams.contains(&term.gram) {
                    *gram_counts.entry(term.gram.clone()).or_insert(0.0) += 0.7;
                }
            }

            // Count low-confidence multiword term grams (weight 0.3),
            // skipping grams already in the encoded sentence
            for term in &sentence_grams.low_confidence_multiword_terms {
                if !encoded_grams.contains(&term.gram) {
                    *gram_counts.entry(term.gram.clone()).or_insert(0.0) += 0.3;
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

        freq_entries.sort_by_key(|entry| Reverse(entry.clone()));

        if !freq_entries.is_empty() {
            result.insert(source_id.clone(), freq_entries);
        }
    }

    result
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
        for term in &sentence_grams.multiword_terms {
            if !encoded_grams.contains(&term.gram) {
                *gram_counts.entry(term.gram.clone()).or_insert(0.0) += 0.7;
            }
        }

        // Count low-confidence multiword term grams (weight 0.3),
        // skipping grams already in the encoded sentence
        for term in &sentence_grams.low_confidence_multiword_terms {
            if !encoded_grams.contains(&term.gram) {
                *gram_counts.entry(term.gram.clone()).or_insert(0.0) += 0.3;
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

#[cfg(test)]
mod tests {
    use super::*;
    use language_utils::{Atom, FrequencySourceId, PimsleurLesson, Word, WordType};

    fn gram(text: &str) -> Gram<String> {
        Gram(vec![Atom::Tok(Word {
            text: text.to_string(),
            word_type: WordType::Heteronym(language_utils::Heteronym {
                word: text.to_string(),
                lemma: text.to_string(),
                pos: language_utils::PartOfSpeech::Noun,
            }),
        })])
    }

    #[test]
    fn test_per_source_gram_frequencies_are_sorted_descending() {
        let alpha = gram("alpha");
        let beta = gram("beta");
        let gamma = gram("gamma");

        let encoded_sentences = vec![
            (
                "s1".to_string(),
                SentenceGrams {
                    grams: vec![
                        SentenceGram::Learnable(alpha.clone()),
                        SentenceGram::Learnable(beta.clone()),
                    ],
                    capitalize_first: false,
                    multiword_terms: vec![],
                    low_confidence_multiword_terms: vec![],
                },
            ),
            (
                "s2".to_string(),
                SentenceGrams {
                    grams: vec![SentenceGram::Learnable(alpha.clone())],
                    capitalize_first: false,
                    multiword_terms: vec![],
                    low_confidence_multiword_terms: vec![],
                },
            ),
            (
                "s3".to_string(),
                SentenceGrams {
                    grams: vec![SentenceGram::Learnable(gamma.clone())],
                    capitalize_first: false,
                    multiword_terms: vec![],
                    low_confidence_multiword_terms: vec![],
                },
            ),
        ];

        let source_id = FrequencySourceId::PimsleurLesson(PimsleurLesson {
            level: 1,
            lesson: 1,
        });
        let sentence_to_sources = FxHashMap::from_iter([
            ("s1".to_string(), vec![source_id.clone()]),
            ("s2".to_string(), vec![source_id.clone()]),
            ("s3".to_string(), vec![source_id.clone()]),
        ]);
        let gram_vocabulary = vec![
            GramVocabEntry {
                atoms: alpha.clone(),
                frequency: 2,
            },
            GramVocabEntry {
                atoms: beta.clone(),
                frequency: 1,
            },
            GramVocabEntry {
                atoms: gamma.clone(),
                frequency: 1,
            },
        ];

        let result = compute_per_source_gram_frequencies(
            &encoded_sentences,
            &sentence_to_sources,
            &gram_vocabulary,
        );
        let entries = &result[&source_id];

        assert_eq!(
            entries.iter().map(|entry| entry.count).collect::<Vec<_>>(),
            vec![2, 1, 1]
        );
        assert!(entries.windows(2).all(|w| w[0].count >= w[1].count));
    }
}
