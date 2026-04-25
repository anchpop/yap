use crate::{Context, Deck, PlacementTest};
use language_utils::{Atom, GramDefinition, Heteronym, PartOfSpeech};
use lasso::Spur;
use pav_regression::{IsotonicRegression, Point, SmoothRegression, UnitWeight};

#[derive(serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct PlacementTestWord {
    pub word: String,
    pub definition: String,
}

/// Extract a single heteronym from a gram, if it's a single-word gram.
/// Returns None for multi-word grams.
fn extract_heteronym(
    spur_gram: &language_utils::SpurGram,
    gram_rodeo: &lasso::RodeoReader<language_utils::Gram<Spur>>,
) -> Option<Heteronym<Spur>> {
    let gram = gram_rodeo.resolve(spur_gram);
    let mut heteronym_iter = gram.atoms().iter().filter_map(|atom| {
        if let Atom::Tok(word) = atom {
            word.heteronym()
        } else {
            None
        }
    });
    let first = heteronym_iter.next()?;
    // Only single-heteronym grams are useful for placement test
    if heteronym_iter.next().is_some() {
        return None;
    }
    Some(*first)
}

impl Context {
    /// Convert PlacementTest results into regression points.
    /// Known words get y=1.0, unknown words get y=0.0, matching the [0, 1]
    /// range produced by card reviews (see `CardData::pre_existing_knowledge`).
    pub(crate) fn get_placement_test_points(
        &self,
        placement_test: &PlacementTest,
    ) -> Vec<Point<f32, UnitWeight>> {
        // Each placement test answer gets several points (spaced slightly apart)
        // to give them more weight relative to individual card reviews.
        const POINTS_PER_ANSWER: usize = 5;

        let mut points = Vec::new();

        for word_str in &placement_test.known_words {
            if let Some((_heteronym, freq)) = self.lookup_word(word_str) {
                for i in 0..POINTS_PER_ANSWER {
                    let offset = (i as f32 - (POINTS_PER_ANSWER as f32 - 1.0) / 2.0) * 0.01;
                    points.push(Point::new_with_weight(freq.ease + offset, 1.0, UnitWeight));
                }
            }
        }

        for word_str in &placement_test.unknown_words {
            if let Some((_heteronym, freq)) = self.lookup_word(word_str) {
                for i in 0..POINTS_PER_ANSWER {
                    let offset = (i as f32 - (POINTS_PER_ANSWER as f32 - 1.0) / 2.0) * 0.01;
                    points.push(Point::new_with_weight(freq.ease + offset, 0.0, UnitWeight));
                }
            }
        }

        points
    }
}

impl Context {
    pub(crate) fn is_word_good_for_placement_test(&self, word: &Heteronym<Spur>) -> bool {
        if word.pos == PartOfSpeech::Intj {
            return false;
        }
        let Some(grams) = self.language_pack.heteronym_to_grams.get(word) else {
            return false;
        };
        let Some(gram_def) = grams
            .iter()
            .find_map(|g| self.language_pack.gram_definitions.get(g))
        else {
            return false;
        };
        let GramDefinition::Dictionary(entry) = gram_def else {
            return false;
        };
        let Some(definition) = entry.definitions.first() else {
            return false;
        };
        !definition.cognate && !definition.false_cognate
    }

    /// Get the first native-language definition for a heteronym, if available.
    pub(crate) fn get_first_definition(&self, word: &Heteronym<Spur>) -> Option<String> {
        let grams = self.language_pack.heteronym_to_grams.get(word)?;
        let gram_def = grams
            .iter()
            .find_map(|g| self.language_pack.gram_definitions.get(g))?;
        let GramDefinition::Dictionary(entry) = gram_def else {
            return None;
        };
        entry.definitions.first().map(|d| d.native.clone())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl Deck {
    /// Helper function to find the heteronym with frequency_score closest to the target value
    /// Uses binary search since gram_frequencies is sorted by frequency (descending)
    /// Returns None if all words at that frequency are excluded
    pub(crate) fn find_heteronym_near_frequency_score_for_placement_test(
        &self,
        target_ln_freq: f32,
        excluded_lemmas: &std::collections::HashSet<Spur>,
    ) -> Option<(Heteronym<Spur>, language_utils::Frequency)> {
        let frequencies = &self.context.language_pack.gram_frequencies.entries;
        let gram_rodeo = &self.context.language_pack.gram_rodeo;
        if frequencies.is_empty() {
            return None;
        }

        // Binary search to find the closest frequency_score
        // Note: frequencies are sorted descending, so ln_frequencies are also descending
        let mut left: usize = 0;
        let mut right = frequencies.len();

        while left < right {
            let mid = (left + right) / 2;
            let (_, freq) = frequencies.get_index(mid)?;
            let mid_ln_freq = freq.ease;

            if mid_ln_freq > target_ln_freq {
                left = mid + 1;
            } else {
                right = mid;
            }
        }

        // Now left is the insertion point. Check neighbors to find closest non-excluded word
        let start = left.saturating_sub(50);
        let end = (left + 50).min(frequencies.len());

        (start..end)
            .filter_map(|i| frequencies.get_index(i))
            .filter_map(|(gram_or_phrase, freq)| {
                let heteronym = extract_heteronym(gram_or_phrase, gram_rodeo)?;
                if excluded_lemmas.contains(&heteronym.lemma) {
                    return None;
                }
                if !self.context.is_word_good_for_placement_test(&heteronym) {
                    return None;
                }
                let distance = (freq.ease - target_ln_freq).abs();
                Some((heteronym, *freq, distance))
            })
            .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(lex, freq, _)| (lex, freq))
    }

    /// Get placement test words distributed by likelihood of knowledge
    /// Takes lists of known and unknown words as strings, builds a regression, and returns
    /// words at different knowledge probability levels (1%, 10%, 20%, ..., 99%)
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
    pub fn get_placement_test(
        &self,
        known_words: Vec<String>,
        unknown_words: Vec<String>,
    ) -> Vec<PlacementTestWord> {
        let gram_rodeo = &self.context.language_pack.gram_rodeo;

        // Convert word strings to their most common heteronyms using the helper method
        let mut known: Vec<Heteronym<Spur>> = Vec::new();
        for word_str in &known_words {
            if let Some((heteronym, _freq)) = self.context.lookup_word(word_str) {
                known.push(heteronym);
            }
        }

        let mut unknown: Vec<Heteronym<Spur>> = Vec::new();
        for word_str in &unknown_words {
            if let Some((heteronym, _freq)) = self.context.lookup_word(word_str) {
                unknown.push(heteronym);
            }
        }

        // Get most and least common words (single-heteronym grams only)
        let (_most_common_heteronym, most_common_freq) = match self
            .context
            .language_pack
            .gram_frequencies
            .entries
            .iter()
            .filter_map(|(gop, freq)| extract_heteronym(gop, gram_rodeo).map(|h| (h, freq)))
            .next()
        {
            Some((heteronym, freq)) => (heteronym, freq),
            None => return vec![],
        };

        let (_least_common_heteronym, least_common_freq) = match self
            .context
            .language_pack
            .gram_frequencies
            .entries
            .iter()
            .rev()
            .filter_map(|(gop, freq)| extract_heteronym(gop, gram_rodeo).map(|h| (h, freq)))
            .next()
        {
            Some((heteronym, freq)) => (heteronym, freq),
            None => return vec![],
        };

        // Build set of excluded lexemes:
        // - All words with same frequency as most common word
        // - All words with same frequency as least common word
        // - All input lexemes (known and unknown)
        let mut excluded_lemmas = std::collections::HashSet::new();

        // Exclude all words with the same frequency as the most common (from start)
        for (gop, freq) in self.context.language_pack.gram_frequencies.entries.iter() {
            if freq.count >= most_common_freq.count {
                if let Some(heteronym) = extract_heteronym(gop, gram_rodeo) {
                    excluded_lemmas.insert(heteronym.lemma);
                }
            } else {
                break; // Frequency changed, stop iterating
            }
        }

        // Exclude all words with the same frequency as the least common (from end)
        for (gop, freq) in self
            .context
            .language_pack
            .gram_frequencies
            .entries
            .iter()
            .rev()
        {
            if freq.count <= least_common_freq.count {
                if let Some(heteronym) = extract_heteronym(gop, gram_rodeo) {
                    excluded_lemmas.insert(heteronym.lemma);
                }
            } else {
                break; // Frequency changed, stop iterating
            }
        }

        // Also exclude all input lexemes
        for lexeme in &known {
            excluded_lemmas.insert(lexeme.lemma);
        }
        for lexeme in &unknown {
            excluded_lemmas.insert(lexeme.lemma);
        }

        // Build a set of excluded word strings (just the words, not full lexemes)
        // to filter results at the end
        let excluded_word_strings: std::collections::HashSet<String> = known
            .iter()
            .chain(unknown.iter())
            .map(|heteronym| {
                self.context
                    .language_pack
                    .string_rodeo
                    .resolve(&heteronym.word)
                    .to_string()
            })
            .collect();

        // Build points for regression
        let mut points = Vec::new();

        // Add most common word as known (y = 1.0)
        points.push(Point::new_with_weight(
            most_common_freq.ease,
            1.0,
            UnitWeight,
        ));

        // Add least common word as unknown (y = 0.0)
        points.push(Point::new_with_weight(
            least_common_freq.ease,
            0.0,
            UnitWeight,
        ));

        // Add all known words
        for &heteronym in &known {
            if let Some(grams) = self
                .context
                .language_pack
                .heteronym_to_grams
                .get(&heteronym)
                && let Some(gram) = grams.first()
                && let Some(freq) = self
                    .context
                    .language_pack
                    .gram_frequencies
                    .entries
                    .get(gram)
            {
                points.push(Point::new_with_weight(freq.ease, 1.0, UnitWeight));
            }
        }

        // Add all unknown words
        for &heteronym in &unknown {
            if let Some(grams) = self
                .context
                .language_pack
                .heteronym_to_grams
                .get(&heteronym)
                && let Some(gram) = grams.first()
                && let Some(freq) = self
                    .context
                    .language_pack
                    .gram_frequencies
                    .entries
                    .get(gram)
            {
                points.push(Point::new_with_weight(freq.ease, 0.0, UnitWeight));
            }
        }

        // Need at least 2 points to create regression
        if points.len() < 2 {
            return vec![];
        }

        // Create isotonic regression (ascending: higher frequency -> higher knowledge)
        let regression = match IsotonicRegression::new_ascending(&points) {
            Ok(reg) => reg,
            Err(e) => {
                log::error!("Failed to create regression for placement test: {e:?}");
                return vec![];
            }
        };

        // Calculate smoothing window (10% of max frequency_score)
        let smoothing_window = most_common_freq.ease * 0.1;
        let smooth_regression = SmoothRegression::from_regression(regression, smoothing_window);

        // Target knowledge probabilities
        let target_probabilities = [
            0.99, 0.90, 0.80, 0.70, 0.60, 0.50, 0.40, 0.30, 0.20, 0.10, 0.01,
        ];

        // Invert to get ln_frequencies for each target probability
        let mut result_words = Vec::new();

        for &target_prob in &target_probabilities {
            // Invert the regression to find x for this y value
            if let Some(target_ln_freq) = smooth_regression.invert(target_prob) {
                // Make sure it's within bounds
                if target_ln_freq >= least_common_freq.ease
                    && target_ln_freq <= most_common_freq.ease
                {
                    // Find a heteronym near this frequency_score using binary search
                    if let Some((heteronym, _freq)) = self
                        .find_heteronym_near_frequency_score_for_placement_test(
                            target_ln_freq,
                            &excluded_lemmas,
                        )
                    {
                        // Add to excluded set so we don't use it again
                        excluded_lemmas.insert(heteronym.lemma);

                        // Get the word string and definition
                        let word_str = self
                            .context
                            .language_pack
                            .string_rodeo
                            .resolve(&heteronym.word);
                        let definition = self
                            .context
                            .get_first_definition(&heteronym)
                            .unwrap_or_default();
                        result_words.push(PlacementTestWord {
                            word: word_str.to_string(),
                            definition,
                        });
                    }
                }
            }
        }

        // Filter out any words that match the known/unknown inputs
        result_words
            .into_iter()
            .filter(|pw| !excluded_word_strings.contains(&pw.word))
            .collect()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_placement_test() {
        use crate::Deck;

        let deck = Deck::default();

        // Test with empty lists (should use just most/least common words)
        let result = deck.get_placement_test(vec![], vec![]);
        println!("Placement test with empty lists:");
        println!("  Returned {} words", result.len());
        for (i, pw) in result.iter().enumerate() {
            let freq = deck
                .context
                .lookup_word(&pw.word)
                .map(|(_, f)| f.count)
                .unwrap_or(0);
            println!(
                "  {}. {} = \"{}\" (freq: {})",
                i + 1,
                pw.word,
                pw.definition,
                freq
            );
        }
        assert!(result.len() <= 11, "Should return at most 11 words");

        // Test with some known and unknown words (just use simple strings now)
        let known = vec![
            "le".to_string(),
            "et".to_string(),
            "pain".to_string(),
            "souvent".to_string(),
            "aller".to_string(),
            "es".to_string(),
            "des".to_string(),
            "a".to_string(),
            "est".to_string(),
        ];

        let unknown = vec!["abandonnés".to_string(), "allées".to_string()];

        let result = deck.get_placement_test(known, unknown);
        println!(
            "\nPlacement test with known=['le', 'et', 'pain', 'souvent', 'aller', 'es', 'des', 'a', 'est'] and unknown=['abandonnés', 'allées']:"
        );
        println!("  Returned {} words", result.len());
        for (i, pw) in result.iter().enumerate() {
            let freq = deck
                .context
                .lookup_word(&pw.word)
                .map(|(_, f)| f.count)
                .unwrap_or(0);
            println!(
                "  {}. {} = \"{}\" (freq: {})",
                i + 1,
                pw.word,
                pw.definition,
                freq
            );
        }
        assert!(result.len() <= 11, "Should return at most 11 words");
        assert!(!result.is_empty(), "Should return at least some words");

        // Verify no duplicates in result
        let unique_words: std::collections::HashSet<_> = result.iter().map(|pw| &pw.word).collect();
        assert_eq!(
            unique_words.len(),
            result.len(),
            "Result should not contain duplicate words"
        );

        println!("\n✓ Placement test completed successfully");
    }
}
