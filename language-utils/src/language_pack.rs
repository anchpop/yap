use crate::indexmap::IndexMap;
use crate::{
    Atom, ConsolidatedLanguageData, Frequency, Gram, GramDefinition, Heteronym, HomophonePractice,
    HomophoneWordPair, Language, Lexeme, Literal, MovieMetadata, PatternPosition,
    PronunciationData, ProperNounDefinition, SentenceGram, SentenceGrams, SentenceSource, SpurGram,
    WordType,
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

    pub fn new(language_data: ConsolidatedLanguageData, target_language: Language) -> Self {
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
        let gram_frequencies: IndexMap<SpurGram, Frequency> = {
            let mut map = IndexMap::new();
            for entry in &language_data.gram_frequencies {
                let interned_gram = entry.gram.get_interned(&rodeo);
                if let Some(interned_gram) = interned_gram
                    && let Some(gram_spur) = gram_rodeo.get(&interned_gram)
                {
                    map.insert(gram_spur, Frequency { count: entry.count });
                }
            }
            map
        };

        // Build unified gram definitions map
        let gram_definitions: FxHashMap<SpurGram, GramDefinition> = {
            let mut map = FxHashMap::default();

            // Add dictionary entries (single-atom grams)
            for (heteronym, definition) in &language_data.gram_dictionary {
                let interned_heteronym = heteronym.get_interned(&rodeo).unwrap_or_else(|| {
                    panic!("gram dictionary heteronym not in rodeo: {heteronym:?}")
                });
                // Create a single-atom gram from this heteronym
                let word = crate::Word {
                    text: interned_heteronym.word,
                    word_type: WordType::Heteronym(interned_heteronym),
                };
                let gram = Gram::new(vec![Atom::Tok(word)]);
                if let Some(gram_spur) = gram_rodeo.get(&gram) {
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
                    v.sort_by(|a, b| b.1.cmp(&a.1));
                    (k, v.into_iter().map(|(spur, _)| spur).collect())
                })
                .collect()
        };

        // Propagate dictionary definitions to all surface-form variants of the same heteronym.
        // E.g., "le" (det) has a definition, but "l'" (det, same heteronym) also needs one.
        let gram_definitions = {
            let mut map = gram_definitions;
            for gram_spurs in heteronym_to_grams.values() {
                // Find the definition from any variant of this heteronym
                let definition = gram_spurs.iter().find_map(|spur| map.get(spur).cloned());
                if let Some(definition) = definition {
                    for &gram_spur in gram_spurs {
                        map.entry(gram_spur).or_insert_with(|| definition.clone());
                    }
                }
            }
            map
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
                            map.insert(gram_spur, Frequency { count: entry.count });
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
                            .max()
                    })
                    .max()?;
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
