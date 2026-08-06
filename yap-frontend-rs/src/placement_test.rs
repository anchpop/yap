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
///
/// The whole gram must be one atom, not merely contain one *heteronym*.
/// Counting heteronyms alone lets a phrase whose other atoms are literals or
/// proper nouns through — French has `doux Jésus`, which would be offered to
/// the learner as the word "doux" while carrying the phrase's ease (6.29)
/// rather than the word's own (4.29). The caller pairs the returned heteronym
/// with the entry's `Frequency`, so those two must describe the same thing.
fn extract_heteronym(
    spur_gram: &language_utils::SpurGram,
    gram_rodeo: &lasso::RodeoReader<language_utils::Gram<Spur>>,
) -> Option<Heteronym<Spur>> {
    let gram = gram_rodeo.resolve(spur_gram);
    let [atom] = gram.atoms() else {
        return None;
    };
    let Atom::Tok(word) = atom else {
        return None;
    };
    word.heteronym().copied()
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
    /// Find the placement-test-eligible heteronym whose `ease` is closest to
    /// `target_ease`. Returns None if no eligible word exists.
    ///
    /// This scans the whole frequency list instead of binary searching it.
    /// `gram_frequencies.entries` is ordered by *count* descending, but `ease`
    /// is not a function of count: single-atom cognates get a flat bonus, and a
    /// multi-atom gram inherits the ease of its hardest component. In French
    /// that leaves ~39% of adjacent pairs inverted with respect to `ease`, so a
    /// binary search keyed on `ease` lands at an arbitrary index — it used to
    /// return a word 3.4 ease away from the target when one within 0.02
    /// existed, which silently collapsed the placement test to a narrow
    /// low-frequency band.
    ///
    /// Candidates are also required to round-trip through `lookup_word`. The
    /// caller only returns the heteronym's surface string, and the answer is
    /// later scored by looking that string back up — which resolves to the
    /// form's *most common* heteronym. Offering a minority heteronym of an
    /// ambiguous form (French `nains` has entries at ease 0.69 and 1.79) would
    /// therefore select at one ease and score at another, quietly feeding the
    /// knowledge regression a point it never asked for.
    pub(crate) fn find_heteronym_near_frequency_score_for_placement_test(
        &self,
        target_ease: f32,
        excluded_lemmas: &std::collections::HashSet<Spur>,
    ) -> Option<(Heteronym<Spur>, language_utils::Frequency)> {
        let frequencies = &self.context.language_pack.gram_frequencies.entries;
        let gram_rodeo = &self.context.language_pack.gram_rodeo;
        let string_rodeo = &self.context.language_pack.string_rodeo;

        frequencies
            .iter()
            .filter_map(|(gram_or_phrase, _freq)| {
                let heteronym = extract_heteronym(gram_or_phrase, gram_rodeo)?;
                if excluded_lemmas.contains(&heteronym.lemma) {
                    return None;
                }
                if !self.context.is_word_good_for_placement_test(&heteronym) {
                    return None;
                }
                // Rank on the frequency `lookup_word` will return for this
                // surface form, not on this entry's own. The list is only an
                // enumeration of candidates: a form can appear in it more than
                // once (French `nains` has entries at ease 0.69 and 1.79), and
                // scoring always goes through `lookup_word`. Taking the ease
                // from the same place the scorer does makes the mismatch
                // unrepresentable rather than merely unlikely.
                let word = string_rodeo.resolve(&heteronym.word);
                let (resolved, resolved_freq) = self.context.lookup_word(word)?;
                if resolved != heteronym {
                    return None;
                }
                let distance = (resolved_freq.ease - target_ease).abs();
                Some((resolved, resolved_freq, distance))
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

        // Invert to get the target ease for each target probability
        let mut result_words = Vec::new();

        for &target_prob in &target_probabilities {
            // Invert the regression to find x for this y value
            if let Some(target_ease) = smooth_regression.invert(target_prob) {
                // Make sure it's within bounds
                if target_ease >= least_common_freq.ease && target_ease <= most_common_freq.ease {
                    // Find the eligible heteronym closest to this ease
                    if let Some((heteronym, _freq)) = self
                        .find_heteronym_near_frequency_score_for_placement_test(
                            target_ease,
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

    /// The placement test picks each word by inverting a regression to a target
    /// ease and then looking up the closest eligible word. When that lookup is
    /// broken it does not fail — it silently returns words clustered in a
    /// narrow low-frequency band, which is useless for placing a learner but
    /// otherwise invisible. (It was once a binary search keyed on `ease` over a
    /// list sorted by *count*; the two orders disagree on ~39% of adjacent
    /// pairs, so the search landed at an arbitrary index.) Assert the spread
    /// directly, so a regression here reads as a selection bug rather than
    /// surfacing downstream as a regression-calibration failure.
    #[test]
    fn test_placement_test_words_span_the_frequency_range() {
        use super::extract_heteronym;
        use crate::Deck;

        let deck = Deck::default();
        let entries = &deck.context.language_pack.gram_frequencies.entries;
        if entries.is_empty() {
            return;
        }
        let gram_rodeo = &deck.context.language_pack.gram_rodeo;

        // The ease the selection is anchored to: the most common single-word gram.
        let most_common_ease = entries
            .iter()
            .find_map(|(gop, freq)| extract_heteronym(gop, gram_rodeo).map(|_| freq.ease))
            .expect("corpus should contain at least one single-word gram");

        let words = deck.get_placement_test(vec![], vec![]);
        assert!(
            words.len() >= 6,
            "expected a usable number of placement words, got {}",
            words.len()
        );

        let eases: Vec<f32> = words
            .iter()
            .filter_map(|w| deck.context.lookup_word(&w.word).map(|(_, f)| f.ease))
            .collect();
        let max_ease = eases.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min_ease = eases.iter().copied().fold(f32::INFINITY, f32::min);

        // The easiest word offered has to be a genuinely common one. The very
        // most common words are deliberately excluded from the test, so some
        // gap is expected — but not the ~4.8 (≈120x rarer) that the broken
        // search produced.
        assert!(
            max_ease >= most_common_ease - 2.5,
            "placement test's easiest word (ease={max_ease:.3}) should be near the most \
             common word (ease={most_common_ease:.3}); a large gap means word selection \
             is not finding words near its target ease"
        );
        assert!(
            max_ease - min_ease >= 4.0,
            "placement test should span a wide difficulty range; got [{min_ease:.3}, {max_ease:.3}]"
        );
    }

    /// The property the selection actually owes its caller: whatever it returns
    /// must be the *closest* eligible candidate to the requested ease. This is
    /// threshold-free and data-independent, so unlike the spread test above it
    /// stays meaningful as the corpus changes.
    #[test]
    fn test_placement_test_selection_returns_the_nearest_eligible_word() {
        use super::extract_heteronym;
        use crate::Deck;

        let deck = Deck::default();
        let entries = &deck.context.language_pack.gram_frequencies.entries;
        if entries.is_empty() {
            return;
        }
        let gram_rodeo = &deck.context.language_pack.gram_rodeo;
        let excluded = std::collections::HashSet::new();

        for target in [9.0f32, 8.0, 6.5, 5.0, 3.5, 2.0, 0.5] {
            let (chosen, chosen_freq) = deck
                .find_heteronym_near_frequency_score_for_placement_test(target, &excluded)
                .expect("corpus should always offer some eligible word");
            let chosen_distance = (chosen_freq.ease - target).abs();

            // Brute force the same question over every entry, applying the same
            // eligibility rules the helper does.
            let best = entries
                .iter()
                .filter_map(|(gop, _freq)| {
                    let h = extract_heteronym(gop, gram_rodeo)?;
                    if !deck.context.is_word_good_for_placement_test(&h) {
                        return None;
                    }
                    let w = deck.context.language_pack.string_rodeo.resolve(&h.word);
                    let (resolved, resolved_freq) = deck.context.lookup_word(w)?;
                    (resolved == h).then(|| (resolved_freq.ease - target).abs())
                })
                .fold(f32::INFINITY, f32::min);

            assert!(
                (chosen_distance - best).abs() < 1e-6,
                "target {target}: selection returned a word {chosen_distance:.4} away \
                 (ease {:.4}) when one {best:.4} away exists",
                chosen_freq.ease,
            );

            // The caller hands the bare surface string back to `lookup_word` to
            // score the answer, so the ease we selected on has to be the ease
            // that lookup returns — otherwise the placement test shows one word
            // and scores another.
            let word = deck
                .context
                .language_pack
                .string_rodeo
                .resolve(&chosen.word);
            let looked_up = deck
                .context
                .lookup_word(word)
                .map(|(_, f)| f.ease)
                .expect("selected word should resolve");
            assert!(
                (looked_up - chosen_freq.ease).abs() < 1e-6,
                "target {target}: selected {word:?} at ease {:.4}, but looking the word \
                 up gives {looked_up:.4} — it would be scored as a different heteronym",
                chosen_freq.ease,
            );
        }
    }

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
