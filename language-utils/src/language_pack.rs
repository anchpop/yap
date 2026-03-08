use crate::indexmap::IndexMap;
use crate::{
    Atom, ConsolidatedLanguageData, Course, DictionaryEntry, Frequency, Gram, GramDefinition,
    Heteronym, HomophonePractice, HomophoneWordPair, Lexeme, Literal, MovieMetadata, PartOfSpeech,
    PatternPosition, PronunciationData, ProperNounDefinition, SentenceGram, SentenceGrams,
    SentenceSource, SpurGram, WordType, grm,
};
use lasso::Spur;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LanguagePack {
    pub string_rodeo: lasso::RodeoReader,
    pub gram_rodeo: lasso::RodeoReader<Gram<Spur>>,
    pub translations: FxHashMap<Spur, Vec<Spur>>,
    pub words_to_heteronyms: FxHashMap<Spur, Vec<Heteronym<Spur>>>,
    pub total_word_count: u64,
    /// Per-movie gram frequencies indexed by movie ID
    pub movie_gram_frequencies: FxHashMap<String, IndexMap<SpurGram, Frequency>>,
    pub word_to_pronunciation: FxHashMap<Spur, Spur>,
    pub pronunciation_to_words: FxHashMap<Spur, Vec<Spur>>,
    pub pronunciation_data: PronunciationData,
    pub pattern_frequency_map: FxHashMap<(Spur, PatternPosition), u32>,
    pub homophone_practice: FxHashMap<HomophoneWordPair<Spur>, HomophonePractice<Spur>>,
    /// Cache of maximum frequencies for each pronunciation (pre-computed at initialization)
    pub pronunciation_max_freq_cache: FxHashMap<Spur, Frequency>,
    /// Movie metadata indexed by movie ID
    pub movies: FxHashMap<String, MovieMetadata>,
    /// Sentence source provenance tracking (maps sentence to its sources)
    pub sentence_sources: FxHashMap<Spur, SentenceSource>,
    /// Global proper noun definitions map
    pub proper_noun_definitions: BTreeMap<Spur, ProperNounDefinition>,
    /// Gram frequencies: maps gram to frequency, for learnable grams
    pub gram_frequencies: IndexMap<SpurGram, Frequency>,
    /// Encoded sentences: maps sentence to grams with learnability and capitalize_first
    /// The gram Spur is a key into gram_rodeo
    pub encoded_sentences: FxHashMap<Spur, SentenceGrams<SpurGram>>,
    /// Gram definitions: dictionary entries (single-word) and phrasebook entries (multi-word)
    /// The Spur is a key into gram_rodeo
    pub gram_definitions: FxHashMap<SpurGram, GramDefinition>,
    /// Index from heteronym to all grams composed only of that heteronym, sorted by frequency (most common first)
    pub heteronym_to_grams: FxHashMap<Heteronym<Spur>, Vec<SpurGram>>,
    /// Index from gram to sentences containing it
    pub sentences_containing_gram_index: FxHashMap<SpurGram, Vec<Spur>>,
    /// Reverse index from display string to grams (for O(1) lookup by phrase text)
    pub string_to_grams: FxHashMap<String, Vec<SpurGram>>,
}

impl LanguagePack {
    /// Get all lexemes for words that share a pronunciation
    /// Returns an iterator over (word, lexeme) pairs
    pub fn pronunciation_to_lexemes(
        &self,
        pronunciation: &Spur,
    ) -> impl Iterator<Item = (Spur, Lexeme<Spur>)> + '_ {
        self.pronunciation_to_words
            .get(pronunciation)
            .into_iter()
            .flat_map(|words| words.iter())
            .flat_map(move |word| {
                self.words_to_heteronyms
                    .get(word)
                    .into_iter()
                    .flat_map(|heteronyms| heteronyms.iter())
                    .map(move |heteronym| {
                        (
                            *word,
                            Lexeme::Heteronym {
                                heteronym: *heteronym,
                            },
                        )
                    })
            })
    }

    /// Derive flat literals for a sentence from its encoded gram representation.
    /// This computes the literals on-the-fly from `encoded_sentences` rather than
    /// storing a separate precomputed map.
    pub fn sentence_to_literals(
        &self,
        sentence: &Spur,
        language: crate::Language,
    ) -> Option<Vec<Literal<String>>> {
        let sentence_grams = self.encoded_sentences.get(sentence)?;

        // Collect all words from gram atoms
        let mut all_words: Vec<crate::Word<String>> = Vec::new();
        for gram in &sentence_grams.grams {
            let spur_gram = match gram {
                SentenceGram::Learnable(g) | SentenceGram::Obvious(g) => g,
            };
            let gram_resolved = self
                .gram_rodeo
                .resolve(spur_gram)
                .resolve(&self.string_rodeo);
            for atom in gram_resolved.iter() {
                if let Atom::Tok(word) = atom {
                    all_words.push(word.clone());
                }
            }
        }

        // Capitalize first word if needed
        if sentence_grams.capitalize_first
            && let Some(first_word) = all_words.first_mut()
        {
            first_word.text = crate::capitalize_first_letter(&first_word.text);
        }

        // Build literals with whitespace prediction
        let literals = all_words
            .iter()
            .enumerate()
            .map(|(i, word)| {
                let next_word = all_words.get(i + 1);
                let whitespace = crate::predict_whitespace(word, next_word, language);
                Literal {
                    word: word.clone(),
                    whitespace: whitespace.to_str().to_string(),
                }
            })
            .collect();

        Some(literals)
    }

    /// Get the maximum frequency for any word with this pronunciation
    pub fn pronunciation_max_frequency(&self, pronunciation: &Spur) -> Option<Frequency> {
        self.pronunciation_max_freq_cache
            .get(pronunciation)
            .copied()
    }

    pub fn new(language_data: ConsolidatedLanguageData, course: Course) -> Self {
        let target_language = course.target_language;
        let rodeo = {
            let mut rodeo = lasso::Rodeo::new();
            language_data.intern(&mut rodeo);
            rodeo.into_reader()
        };

        let translations = {
            language_data
                .translations
                .iter()
                .map(|(target_language, native_languages)| {
                    (
                        rodeo.get(target_language).unwrap(),
                        native_languages
                            .iter()
                            .map(|n| rodeo.get(n).unwrap())
                            .collect(),
                    )
                })
                .collect()
        };

        let words_to_heteronyms: FxHashMap<Spur, Vec<Heteronym<Spur>>> = {
            let mut map: FxHashMap<Spur, Vec<(Heteronym<Spur>, u32)>> = FxHashMap::default();

            for entry in &language_data.gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    let word_spur = rodeo.get(&heteronym.word).unwrap();
                    let interned_het = Heteronym {
                        word: rodeo.get(&heteronym.word).unwrap(),
                        lemma: rodeo.get(&heteronym.lemma).unwrap(),
                        pos: heteronym.pos,
                    };
                    let vec = map.entry(word_spur).or_default();
                    if let Some(existing) = vec.iter_mut().find(|(h, _)| *h == interned_het) {
                        existing.1 = existing.1.max(entry.count);
                    } else {
                        vec.push((interned_het, entry.count));
                    }
                }
            }

            // Sort each list by frequency descending, then strip the frequencies
            map.into_iter()
                .map(|(word, mut hets)| {
                    hets.sort_by(|a, b| b.1.cmp(&a.1));
                    (word, hets.into_iter().map(|(h, _)| h).collect::<Vec<_>>())
                })
                .collect()
        };

        let total_word_count = {
            language_data
                .gram_frequencies
                .iter()
                .map(|entry| entry.count as u64)
                .sum()
        };

        let word_to_pronunciation = {
            language_data
                .word_to_pronunciation
                .iter()
                .map(|(word, pronunciation)| {
                    (
                        rodeo
                            .get(word)
                            .unwrap_or_else(|| panic!("word not in rodeo: {word:?}")),
                        rodeo.get(pronunciation).unwrap_or_else(|| {
                            panic!("pronunciation not in rodeo: {pronunciation:?}")
                        }),
                    )
                })
                .collect()
        };

        let pronunciation_to_words: FxHashMap<Spur, Vec<Spur>> = {
            language_data
                .pronunciation_to_words
                .iter()
                .map(|(pronunciation, words)| {
                    (
                        rodeo.get(pronunciation).unwrap(),
                        words.iter().map(|word| rodeo.get(word).unwrap()).collect(),
                    )
                })
                .collect()
        };

        let pronunciation_data = language_data.pronunciation_data.clone();

        let pattern_frequency_map = {
            pronunciation_data
                .pattern_frequencies
                .iter()
                .map(|((pattern, position), freq)| {
                    ((rodeo.get(pattern).unwrap(), *position), *freq)
                })
                .collect()
        };

        let homophone_practice = language_data
            .homophone_practice
            .iter()
            .map(|(word_pair, practice)| {
                (
                    word_pair.get_interned(&rodeo).unwrap(),
                    practice.get_interned(&rodeo).unwrap(),
                )
            })
            .collect();

        // Initialize movie data
        let movies = language_data.movies;

        // Store movie_gram_frequencies to convert after gram_rodeo is created
        let movie_gram_frequencies_data = language_data.movie_gram_frequencies.clone();

        // Convert sentence sources
        let sentence_sources = {
            language_data
                .sentence_sources
                .iter()
                .map(|(sentence, source)| (rodeo.get(sentence).unwrap(), source.clone()))
                .collect()
        };

        // Convert proper noun definitions to use Spurs
        let proper_noun_definitions = {
            language_data
                .proper_noun_definitions
                .iter()
                .map(|(proper_noun, definition)| {
                    (rodeo.get(proper_noun).unwrap(), definition.clone())
                })
                .collect()
        };

        // Convert gram vocabulary entries to use Spurs
        let gram_vocabulary: Vec<_> = language_data
            .gram_vocabulary
            .iter()
            .map(|entry| {
                entry
                    .get_interned(&rodeo)
                    .expect("all gram vocab atoms should be interned")
            })
            .collect();

        // Intern all grams into the gram_rodeo
        let gram_rodeo = {
            let mut gram_rodeo: lasso::Rodeo<Gram<Spur>> = lasso::Rodeo::new();
            for entry in &gram_vocabulary {
                gram_rodeo.get_or_intern(entry.atoms.clone());
            }
            gram_rodeo.into_reader()
        };

        // Convert encoded sentences to use Spurs for sentence keys
        // The grams are looked up in gram_rodeo to get their Spur keys
        // multiword_terms and low_confidence_multiword_terms are now Gram<String>,
        // so they need to be interned into gram_rodeo as SpurGram
        let encoded_sentences: FxHashMap<Spur, SentenceGrams<SpurGram>> = language_data
            .encoded_sentences
            .iter()
            .filter_map(|(sentence, encoded)| {
                let interned_grams: Option<Vec<SentenceGram<SpurGram>>> = encoded
                    .grams
                    .iter()
                    .map(|g| {
                        g.get_interned(&rodeo)?
                            .try_map(|gram| gram_rodeo.get(&gram))
                    })
                    .collect();
                let interned_multiword_terms: Option<Vec<SpurGram>> = encoded
                    .multiword_terms
                    .iter()
                    .map(|term| {
                        let interned = term.get_interned(&rodeo)?;
                        interned.get_interned(&gram_rodeo)
                    })
                    .collect();
                let interned_low_confidence_multiword_terms: Option<Vec<SpurGram>> = encoded
                    .low_confidence_multiword_terms
                    .iter()
                    .map(|term| {
                        let interned = term.get_interned(&rodeo)?;
                        interned.get_interned(&gram_rodeo)
                    })
                    .collect();
                Some((
                    rodeo.get(sentence)?,
                    SentenceGrams::<SpurGram> {
                        grams: interned_grams?,
                        capitalize_first: encoded.capitalize_first,
                        multiword_terms: interned_multiword_terms?,
                        low_confidence_multiword_terms: interned_low_confidence_multiword_terms?,
                    },
                ))
            })
            .collect();

        // Convert gram frequencies (pre-computed in generate-data)
        // Each entry now has a `gram` field (Gram<String>) which we intern
        // Build as counts first; we'll compute the `easy` flag after gram_definitions is available.
        let gram_counts: IndexMap<SpurGram, (u32, u32)> = {
            let mut map = IndexMap::new();
            for entry in &language_data.gram_frequencies {
                let interned_gram = entry.gram.get_interned(&rodeo);
                if let Some(interned_gram) = interned_gram
                    && let Some(gram_spur) = gram_rodeo.get(&interned_gram)
                {
                    map.insert(gram_spur, (entry.count, entry.direct_count));
                }
            }
            map
        };

        // Build unified gram definitions map
        let gram_definitions: FxHashMap<SpurGram, GramDefinition> = {
            let mut map = FxHashMap::default();

            // Add dictionary entries (keyed by Gram directly)
            for (gram, definition) in &language_data.gram_dictionary {
                let interned_gram: Option<Gram<Spur>> =
                    gram.iter().map(|atom| atom.get_interned(&rodeo)).collect();
                if let Some(interned_gram) = interned_gram
                    && let Some(gram_spur) = gram_rodeo.get(&interned_gram)
                {
                    map.insert(gram_spur, GramDefinition::Dictionary(definition.clone()));
                }
            }

            // Add phrasebook entries (multi-atom grams)
            for (gram, entry) in &language_data.phrasebook {
                let interned_gram: Option<Gram<Spur>> =
                    gram.iter().map(|atom| atom.get_interned(&rodeo)).collect();
                if let Some(interned_gram) = interned_gram
                    && let Some(gram_spur) = gram_rodeo.get(&interned_gram)
                {
                    map.insert(gram_spur, GramDefinition::Phrasebook(entry.clone()));
                }
            }

            map
        };

        // Now that we have definitions, convert gram_counts into gram_frequencies with `easy` and `ease` computed.
        let gram_frequencies: IndexMap<SpurGram, Frequency> = {
            const COGNATE_BONUS: f32 = 2.0;

            // First pass: compute base ease for single-atom grams so we can reference them
            // when computing multi-atom gram ease.
            let mut single_atom_ease: FxHashMap<Atom<Spur>, f32> = FxHashMap::default();
            for (gram_spur, (count, _direct_count)) in gram_counts.iter() {
                let gram = gram_rodeo.resolve(gram_spur);
                if gram.len() == 1 {
                    if let Some(atom) = gram.iter().next() {
                        let base_ease = (*count as f32).ln();
                        let has_cognate = !course.teaches_new_writing_system()
                            && has_cognate_definition(gram_definitions.get(gram_spur));
                        let ease = if has_cognate {
                            base_ease + COGNATE_BONUS
                        } else {
                            base_ease
                        };
                        single_atom_ease.insert(*atom, ease);
                    }
                }
            }

            // Second pass: build frequency entries with ease for all grams.
            let mut map = IndexMap::new();
            for (gram_spur, (count, direct_count)) in gram_counts.iter() {
                let gram = gram_rodeo.resolve(gram_spur);
                let easy = !course.teaches_new_writing_system()
                    && is_gram_easy(gram, gram_definitions.get(gram_spur));

                let ln_count = (*count as f32).ln();

                // Extract phrasebook properties for multi-atom grams
                let definition = gram_definitions.get(gram_spur);
                let (is_compositional, is_literally_translatable, is_cognate) = match definition {
                    Some(GramDefinition::Phrasebook(entry)) => (
                        entry.compositional,
                        entry.can_be_translated_literally,
                        entry.cognate && !entry.false_cognate,
                    ),
                    _ => (false, false, false),
                };
                let compositional = is_compositional || is_literally_translatable;

                let ease = if gram.len() <= 1 {
                    // Single-atom gram: use pre-computed ease from first pass.
                    gram.iter()
                        .next()
                        .and_then(|atom| single_atom_ease.get(atom).copied())
                        .unwrap_or(ln_count)
                } else {
                    // Multi-atom gram: ease depends on compositionality and component word ease.
                    let min_component_ease = gram
                        .iter()
                        .filter_map(|atom| single_atom_ease.get(atom).copied())
                        .reduce(f32::min);

                    let base_ease = match (
                        is_literally_translatable,
                        is_compositional,
                        min_component_ease,
                    ) {
                        // Can be translated word-for-word: as easy as the hardest component.
                        (true, _, Some(min_ease)) => min_ease.max(ln_count),
                        // Compositional but not literal: blend component ease with own frequency.
                        (false, true, Some(min_ease)) => {
                            (0.7 * min_ease + 0.3 * ln_count).max(ln_count)
                        }
                        // Non-compositional (idiom) or no component data: own frequency only.
                        _ => ln_count,
                    };

                    let cognate_bonus = if is_cognate && !course.teaches_new_writing_system() {
                        COGNATE_BONUS
                    } else {
                        0.0
                    };

                    base_ease + cognate_bonus
                };

                map.insert(
                    *gram_spur,
                    Frequency {
                        count: *count,
                        direct_count: *direct_count,
                        easy,
                        compositional,
                        ease,
                    },
                );
            }
            map
        };

        // Build index from heteronym to all grams composed only of that heteronym, sorted by frequency
        let heteronym_to_grams: FxHashMap<Heteronym<Spur>, Vec<SpurGram>> = {
            let mut map: FxHashMap<Heteronym<Spur>, Vec<(SpurGram, Frequency)>> =
                FxHashMap::default();
            for (gram_spur, freq) in gram_frequencies.iter() {
                let gram = gram_rodeo.resolve(gram_spur);
                // Check if this gram is composed of a single heteronym atom
                if gram.len() == 1
                    && let Some(Atom::Tok(word)) = gram.iter().next()
                    && let WordType::Heteronym(heteronym) = &word.word_type
                {
                    map.entry(*heteronym).or_default().push((*gram_spur, *freq));
                }
            }
            // Sort each list by frequency (highest first) and extract just the spurs
            map.into_iter()
                .map(|(k, mut v)| {
                    v.sort_by(|a, b| b.1.count.cmp(&a.1.count));
                    (k, v.into_iter().map(|(spur, _)| spur).collect())
                })
                .collect()
        };

        // Build index from gram to sentences containing it
        // All grams, multiword_terms, and low_confidence_multiword_terms are now unified as SpurGram
        let sentences_containing_gram_index: FxHashMap<SpurGram, Vec<Spur>> = {
            let mut map: FxHashMap<SpurGram, Vec<Spur>> = FxHashMap::default();
            for (sentence_spur, sentence_grams) in encoded_sentences.iter() {
                // Add grams to index
                for gram in &sentence_grams.grams {
                    let gram_spur = match gram {
                        SentenceGram::Learnable(g) | SentenceGram::Obvious(g) => *g,
                    };
                    map.entry(gram_spur).or_default().push(*sentence_spur);
                }
                // Add high-confidence multiword term grams to index
                for gram_spur in &sentence_grams.multiword_terms {
                    map.entry(*gram_spur).or_default().push(*sentence_spur);
                }
                // Add low-confidence multiword term grams to index
                for gram_spur in &sentence_grams.low_confidence_multiword_terms {
                    map.entry(*gram_spur).or_default().push(*sentence_spur);
                }
            }
            map
        };

        // Convert per-movie gram frequencies (pre-computed in generate-data)
        // Each entry now has a `gram` field (Gram<String>) which we intern
        let movie_gram_frequencies: FxHashMap<String, IndexMap<SpurGram, Frequency>> =
            movie_gram_frequencies_data
                .iter()
                .map(|(movie_id, freqs)| {
                    let mut map: IndexMap<SpurGram, Frequency> = IndexMap::new();
                    for entry in freqs {
                        let interned_gram = entry.gram.get_interned(&rodeo);
                        if let Some(interned_gram) = interned_gram
                            && let Some(gram_spur) = gram_rodeo.get(&interned_gram)
                        {
                            // Use ease from global gram_frequencies; fall back to ln(count)
                            let global_freq = gram_frequencies.get(&gram_spur);
                            map.insert(
                                gram_spur,
                                Frequency {
                                    count: entry.count,
                                    direct_count: entry.direct_count,
                                    easy: global_freq.is_some_and(|f| f.easy),
                                    compositional: global_freq.is_some_and(|f| f.compositional),
                                    ease: global_freq
                                        .map(|f| f.ease)
                                        .unwrap_or_else(|| (entry.count as f32).ln()),
                                },
                            );
                        }
                    }
                    (movie_id.clone(), map)
                })
                .filter(|(_, map)| !map.is_empty())
                .collect();

        // Pre-compute pronunciation max frequencies for performance
        // Look up through heteronym_to_grams -> gram_frequencies
        let pronunciation_max_freq_cache: FxHashMap<Spur, Frequency> = pronunciation_to_words
            .iter()
            .filter_map(|(pronunciation, words)| {
                let max_freq = words
                    .iter()
                    .flat_map(|word| {
                        words_to_heteronyms
                            .get(word)
                            .map(|heteronyms| heteronyms.iter())
                            .into_iter()
                            .flatten()
                    })
                    .filter_map(|heteronym| {
                        // Find the max frequency among all grams for this heteronym
                        heteronym_to_grams
                            .get(heteronym)?
                            .iter()
                            .filter_map(|gram_spur| gram_frequencies.get(gram_spur).copied())
                            .max_by_key(|f| f.count)
                    })
                    .max_by_key(|f| f.count)?;
                Some((*pronunciation, max_freq))
            })
            .collect();

        // Build reverse index from display string to grams
        let string_to_grams: FxHashMap<String, Vec<SpurGram>> = {
            let mut map: FxHashMap<String, Vec<SpurGram>> = FxHashMap::default();
            for &gram_spur in gram_definitions.keys() {
                let resolved = gram_rodeo.resolve(&gram_spur).resolve(&rodeo);
                let display = resolved.to_display_string(target_language);
                map.entry(display).or_default().push(gram_spur);
            }
            map
        };

        Self {
            string_rodeo: rodeo,
            gram_rodeo,
            translations,
            words_to_heteronyms,
            total_word_count,
            movie_gram_frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
            pronunciation_data,
            pattern_frequency_map,
            homophone_practice,
            pronunciation_max_freq_cache,
            movies,
            sentence_sources,
            proper_noun_definitions,
            gram_frequencies,
            encoded_sentences,
            gram_definitions,
            heteronym_to_grams,
            sentences_containing_gram_index,
            string_to_grams,
        }
    }
}

/// Check if a resolved gram is "easy" based on its definition.
/// Easy grams are single-word cognates with a single-word definition.
fn is_gram_easy(gram: &grm<Spur>, definition: Option<&GramDefinition>) -> bool {
    // Must be a single-word gram with a heteronym
    if gram.len() != 1 {
        return false;
    }
    let Some(Atom::Tok(word)) = gram.iter().next() else {
        return false;
    };
    let WordType::Heteronym(h) = &word.word_type else {
        return false;
    };
    if h.pos == PartOfSpeech::Intj {
        return false;
    }

    let Some(GramDefinition::Dictionary(entry)) = definition else {
        return false;
    };
    is_dictionary_entry_easy(entry)
}

/// Check if a gram has any cognate definition (more permissive than is_gram_easy).
pub fn has_cognate_definition(definition: Option<&GramDefinition>) -> bool {
    match definition {
        Some(GramDefinition::Dictionary(entry)) => entry
            .definitions
            .iter()
            .any(|d| d.cognate && !d.false_cognate),
        Some(GramDefinition::Phrasebook(entry)) => entry.cognate && !entry.false_cognate,
        None => false,
    }
}

fn is_dictionary_entry_easy(entry: &DictionaryEntry) -> bool {
    if entry.definitions.len() != 1 {
        return false;
    }
    let Some(definition) = entry.definitions.first() else {
        return false;
    };
    if definition.native.contains(' ') {
        return false;
    }
    if definition.native == entry.target_language_word {
        return false;
    }
    definition.cognate && !definition.false_cognate
}
