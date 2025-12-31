use crate::{Context, Deck, PlacementTest};
use language_utils::Lexeme;
use lasso::Spur;
use pav_regression::{IsotonicRegression, Point, SmoothRegression};

impl Context {
    /// Convert PlacementTest results into regression points
    /// Known words get y=5.0, unknown words get y=0.0
    pub(crate) fn get_placement_test_points(
        &self,
        placement_test: &PlacementTest,
    ) -> Vec<Point<f64>> {
        let mut points = Vec::new();

        // Add points for known words (y = 5.0)
        for word_str in &placement_test.known_words {
            if let Some((_lexeme, freq)) = self.lookup_word(word_str) {
                points.push(Point::new(freq.ln_frequency(), 5.0));
            }
        }

        // Add points for unknown words (y = 0.0)
        for word_str in &placement_test.unknown_words {
            if let Some((_lexeme, freq)) = self.lookup_word(word_str) {
                points.push(Point::new(freq.ln_frequency(), 0.0));
            }
        }

        points
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl Deck {
    /// Helper function to find the lexeme with ln_frequency closest to the target value
    /// Uses binary search since word_frequencies is sorted by frequency (descending)
    /// Returns None if all words at that frequency are excluded
    pub(crate) fn find_lexeme_near_ln_frequency(
        &self,
        target_sqrt_freq: f64,
        excluded_lexemes: &std::collections::HashSet<Lexeme<Spur>>,
        heteronyms_only: bool,
    ) -> Option<(Lexeme<Spur>, language_utils::Frequency)> {
        let frequencies = &self.context.language_pack.word_frequencies;
        if frequencies.is_empty() {
            return None;
        }

        // Binary search to find the closest ln_frequency
        // Note: frequencies are sorted descending, so sqrt_frequencies are also descending
        let mut left = 0;
        let mut right = frequencies.len();

        while left < right {
            let mid = (left + right) / 2;
            let (_, freq) = frequencies.get_index(mid)?;
            let mid_sqrt_freq = freq.ln_frequency();

            if mid_sqrt_freq > target_sqrt_freq {
                left = mid + 1;
            } else {
                right = mid;
            }
        }

        // Now left is the insertion point. Check neighbors to find closest non-excluded word
        let mut best_match: Option<(Lexeme<Spur>, language_utils::Frequency, f64)> = None;

        // Check a range around the insertion point
        let start = left.saturating_sub(5);
        let end = (left + 5).min(frequencies.len());

        for i in start..end {
            if let Some((lexeme, freq)) = frequencies.get_index(i) {
                if excluded_lexemes.contains(lexeme) {
                    continue;
                }

                // Skip multiwords if heteronyms_only is true
                if heteronyms_only && matches!(lexeme, Lexeme::Multiword(_)) {
                    continue;
                }

                let sqrt_freq = freq.ln_frequency();
                let distance = (sqrt_freq - target_sqrt_freq).abs();

                match &best_match {
                    None => best_match = Some((*lexeme, *freq, distance)),
                    Some((_, _, best_distance)) if distance < *best_distance => {
                        best_match = Some((*lexeme, *freq, distance));
                    }
                    _ => {}
                }
            }
        }

        best_match.map(|(lex, freq, _)| (lex, freq))
    }

    /// Get placement test words distributed by likelihood of knowledge
    /// Takes lists of known and unknown words as strings, builds a regression, and returns
    /// words at different knowledge probability levels (1%, 10%, 20%, ..., 99%)
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
    pub fn get_placement_test(
        &self,
        known_words: Vec<String>,
        unknown_words: Vec<String>,
    ) -> Vec<String> {
        // Convert word strings to their most common lexemes using the helper method
        let mut known: Vec<Lexeme<Spur>> = Vec::new();
        for word_str in &known_words {
            if let Some((lexeme, _freq)) = self.context.lookup_word(word_str) {
                known.push(lexeme);
            }
        }

        let mut unknown: Vec<Lexeme<Spur>> = Vec::new();
        for word_str in &unknown_words {
            if let Some((lexeme, _freq)) = self.context.lookup_word(word_str) {
                unknown.push(lexeme);
            }
        }

        // Get most and least common words
        let (_most_common_lexeme, most_common_freq) =
            match self.context.language_pack.word_frequencies.get_index(0) {
                Some((lex, freq)) => (*lex, *freq),
                None => return vec![],
            };

        let (_least_common_lexeme, least_common_freq) =
            match self.context.language_pack.word_frequencies.iter().last() {
                Some((lex, freq)) => (*lex, *freq),
                None => return vec![],
            };

        // Build set of excluded lexemes:
        // - All words with same frequency as most common word
        // - All words with same frequency as least common word
        // - All input lexemes (known and unknown)
        let mut excluded_lexemes = std::collections::HashSet::new();

        // Exclude all words with the same frequency as the most common (from start)
        for (lexeme, freq) in self.context.language_pack.word_frequencies.iter() {
            if freq.count == most_common_freq.count {
                excluded_lexemes.insert(*lexeme);
            } else {
                break; // Frequency changed, stop iterating
            }
        }

        // Exclude all words with the same frequency as the least common (from end)
        for (lexeme, freq) in self.context.language_pack.word_frequencies.iter().rev() {
            if freq.count == least_common_freq.count {
                excluded_lexemes.insert(*lexeme);
            } else {
                break; // Frequency changed, stop iterating
            }
        }

        // Also exclude all input lexemes
        for lexeme in &known {
            excluded_lexemes.insert(*lexeme);
        }
        for lexeme in &unknown {
            excluded_lexemes.insert(*lexeme);
        }

        // Build a set of excluded word strings (just the words, not full lexemes)
        // to filter results at the end
        let excluded_word_strings: std::collections::HashSet<String> = known
            .iter()
            .chain(unknown.iter())
            .map(|lexeme| match lexeme {
                Lexeme::Heteronym(h) => self
                    .context
                    .language_pack
                    .rodeo
                    .resolve(&h.word)
                    .to_string(),
                Lexeme::Multiword(s) => self.context.language_pack.rodeo.resolve(s).to_string(),
            })
            .collect();

        // Build points for regression
        let mut points = Vec::new();

        // Add most common word as known (y = 1.0)
        points.push(Point::new(most_common_freq.ln_frequency(), 1.0));

        // Add least common word as unknown (y = 0.0)
        points.push(Point::new(least_common_freq.ln_frequency(), 0.0));

        // Add all known words
        for lexeme in &known {
            if let Some(freq) = self.context.language_pack.word_frequencies.get(lexeme) {
                points.push(Point::new(freq.ln_frequency(), 1.0));
            }
        }

        // Add all unknown words
        for lexeme in &unknown {
            if let Some(freq) = self.context.language_pack.word_frequencies.get(lexeme) {
                points.push(Point::new(freq.ln_frequency(), 0.0));
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

        // Calculate smoothing window (20% of max ln_frequency)
        let smoothing_window = most_common_freq.ln_frequency() * 0.2;
        let smooth_regression = SmoothRegression::from_regression(&regression, smoothing_window);

        // Target knowledge probabilities
        let target_probabilities = [
            0.99, 0.90, 0.80, 0.70, 0.60, 0.50, 0.40, 0.30, 0.20, 0.10, 0.01,
        ];

        // Invert to get sqrt_frequencies for each target probability
        let mut result_words = Vec::new();

        for &target_prob in &target_probabilities {
            // Invert the regression to find x for this y value
            if let Some(target_sqrt_freq) = smooth_regression.invert(target_prob) {
                // Make sure it's within bounds
                if target_sqrt_freq >= least_common_freq.ln_frequency()
                    && target_sqrt_freq <= most_common_freq.ln_frequency()
                {
                    // Find a lexeme near this ln_frequency using binary search
                    // Use heteronyms_only=true to prefer single words over multiword phrases
                    if let Some((lexeme, _freq)) = self.find_lexeme_near_ln_frequency(
                        target_sqrt_freq,
                        &excluded_lexemes,
                        true,
                    ) {
                        // Add to excluded set so we don't use it again
                        excluded_lexemes.insert(lexeme);

                        // Get the word string
                        let word_str = match lexeme {
                            Lexeme::Heteronym(h) => {
                                self.context.language_pack.rodeo.resolve(&h.word)
                            }
                            Lexeme::Multiword(s) => self.context.language_pack.rodeo.resolve(&s),
                        };
                        result_words.push(word_str.to_string());
                    }
                }
            }
        }

        // Filter out any words that match the known/unknown inputs
        result_words
            .into_iter()
            .filter(|word| !excluded_word_strings.contains(word))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_test() {
        use crate::Deck;

        let deck = Deck::default();

        // Test with empty lists (should use just most/least common words)
        let result = deck.get_placement_test(vec![], vec![]);
        println!("Placement test with empty lists:");
        println!("  Returned {} words", result.len());
        for (i, word) in result.iter().enumerate() {
            // Look up frequency for this word
            let freq = deck
                .context
                .language_pack
                .word_frequencies
                .iter()
                .find(|(lex, _)| {
                    let lex_word = match lex {
                        Lexeme::Heteronym(h) => deck.context.language_pack.rodeo.resolve(&h.word),
                        Lexeme::Multiword(s) => deck.context.language_pack.rodeo.resolve(s),
                    };
                    lex_word == word
                })
                .map(|(_, f)| f.count)
                .unwrap_or(0);
            println!("  {}. {} (freq: {})", i + 1, word, freq);
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
        for (i, word) in result.iter().enumerate() {
            // Look up frequency for this word
            let freq = deck
                .context
                .language_pack
                .word_frequencies
                .iter()
                .find(|(lex, _)| {
                    let lex_word = match lex {
                        Lexeme::Heteronym(h) => deck.context.language_pack.rodeo.resolve(&h.word),
                        Lexeme::Multiword(s) => deck.context.language_pack.rodeo.resolve(s),
                    };
                    lex_word == word
                })
                .map(|(_, f)| f.count)
                .unwrap_or(0);
            println!("  {}. {} (freq: {})", i + 1, word, freq);
        }
        assert!(result.len() <= 11, "Should return at most 11 words");
        assert!(!result.is_empty(), "Should return at least some words");

        // Verify no duplicates in result
        let unique_words: std::collections::HashSet<_> = result.iter().collect();
        assert_eq!(
            unique_words.len(),
            result.len(),
            "Result should not contain duplicate words"
        );

        println!("\n✓ Placement test completed successfully");
    }
}
