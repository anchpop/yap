use crate::indexmap::IndexMap;
use crate::{
    ConsolidatedLanguageData, DictionaryEntry, Frequency, Heteronym, HomophonePractice,
    HomophoneWordPair, Lexeme, Literal, MovieMetadata, PatternPosition, PhrasebookEntry,
    PronunciationData, ProperNounDefinition, SentenceSource,
};
use lasso::Spur;
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LanguagePack {
    pub rodeo: lasso::RodeoReader,
    pub translations: FxHashMap<Spur, Vec<Spur>>,
    pub words_to_heteronyms: FxHashMap<Spur, BTreeSet<Heteronym<Spur>>>,
    pub sentences_containing_lexeme_index: FxHashMap<Lexeme<Spur>, Vec<Spur>>,
    pub sentences_to_literals: FxHashMap<Spur, Vec<Literal<Spur>>>,
    pub sentences_to_lexemes: FxHashMap<Spur, Vec<Lexeme<Spur>>>,
    pub sentences_to_all_lexemes: FxHashMap<Spur, Vec<Lexeme<Spur>>>,
    pub word_frequencies: IndexMap<Lexeme<Spur>, Frequency>,
    pub total_word_count: u64,
    /// Per-movie word frequencies indexed by movie ID
    pub movie_word_frequencies: FxHashMap<String, IndexMap<Lexeme<Spur>, Frequency>>,
    pub dictionary: BTreeMap<Heteronym<Spur>, DictionaryEntry>,
    pub phrasebook: BTreeMap<Spur, PhrasebookEntry>,
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
                    .map(move |heteronym| (*word, Lexeme::Heteronym(*heteronym)))
            })
    }

    /// Get the maximum frequency for any word with this pronunciation
    pub fn pronunciation_max_frequency(&self, pronunciation: &Spur) -> Option<Frequency> {
        self.pronunciation_max_freq_cache
            .get(pronunciation)
            .copied()
    }

    pub fn new(language_data: ConsolidatedLanguageData) -> Self {
        let rodeo = {
            let mut rodeo = lasso::Rodeo::new();
            language_data.intern(&mut rodeo);
            rodeo.into_reader()
        };

        let sentences: Vec<Spur> = {
            language_data
                .target_language_sentences
                .iter()
                .map(|s| rodeo.get(s).unwrap())
                .collect()
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

        let words_to_heteronyms = {
            let mut map: FxHashMap<Spur, BTreeSet<Heteronym<Spur>>> = FxHashMap::default();

            for freq in &language_data.frequencies {
                if let Lexeme::Heteronym(heteronym) = &freq.lexeme {
                    let word_spur = rodeo.get(&heteronym.word).unwrap();
                    map.entry(word_spur).or_default().insert({
                        Heteronym {
                            word: rodeo.get(&heteronym.word).unwrap(),
                            lemma: rodeo.get(&heteronym.lemma).unwrap(),
                            pos: heteronym.pos,
                        }
                    });
                }
            }

            map
        };

        let sentences_to_literals = {
            language_data
                .nlp_sentences
                .iter()
                .map(|(sentence, analysis)| {
                    (
                        rodeo.get(sentence).unwrap(),
                        analysis
                            .words
                            .iter()
                            .map(|word| {
                                word.get_interned(&rodeo).unwrap_or_else(|| {
                                    panic!("word not in rodeo: {word:?} in sentence: {sentence:?}")
                                })
                            })
                            .collect(),
                    )
                })
                .collect()
        };

        let sentences_to_lexemes: FxHashMap<Spur, Vec<Lexeme<Spur>>> = {
            language_data
                .nlp_sentences
                .iter()
                .map(|(sentence, analysis)| {
                    (
                        rodeo.get(sentence).unwrap(),
                        analysis
                            .lexemes()
                            .map(|l| l.get_interned(&rodeo).unwrap())
                            .collect(),
                    )
                })
                .collect()
        };

        let sentences_containing_lexeme_index = {
            let mut map = FxHashMap::default();
            for (i, sentence_spur) in sentences.iter().enumerate() {
                let _sentence = rodeo.resolve(sentence_spur);
                let Some(lexemes) = sentences_to_lexemes.get(sentence_spur) else {
                    continue;
                };
                for lexeme in lexemes.iter().cloned() {
                    map.entry(lexeme).or_insert(vec![]).push(sentences[i]);
                }
            }
            map
        };

        let sentences_to_all_lexemes = {
            language_data
                .nlp_sentences
                .iter()
                .map(|(sentence, analysis)| {
                    (
                        rodeo.get(sentence).unwrap(),
                        analysis
                            .all_lexemes()
                            .map(|l| l.get_interned(&rodeo).unwrap())
                            .collect(),
                    )
                })
                .collect()
        };

        let word_frequencies = {
            let mut map = IndexMap::new();
            for freq in &language_data.frequencies {
                map.insert(
                    freq.lexeme.get_interned(&rodeo).unwrap(),
                    Frequency { count: freq.count },
                );
            }
            map
        };

        let total_word_count = {
            language_data
                .frequencies
                .iter()
                .map(|freq| freq.count as u64)
                .sum()
        };

        let dictionary = {
            language_data
                .dictionary
                .iter()
                .map(|(heteronym, entry)| {
                    (
                        heteronym
                            .get_interned(&rodeo)
                            .unwrap_or_else(|| panic!("heteronym not in rodeo: {heteronym:?}")),
                        entry.clone(),
                    )
                })
                .collect()
        };

        let phrasebook = {
            language_data
                .phrasebook
                .iter()
                .map(|(multiword_term, entry)| (rodeo.get(multiword_term).unwrap(), entry.clone()))
                .collect()
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

        // Pre-compute pronunciation max frequencies for performance
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
                        let lexeme = Lexeme::Heteronym(*heteronym);
                        word_frequencies.get(&lexeme).copied()
                    })
                    .max()?;
                Some((*pronunciation, max_freq))
            })
            .collect();

        // Initialize movie data
        let movies = language_data.movies;

        // Convert per-movie frequencies
        let movie_word_frequencies = {
            language_data
                .movie_frequencies
                .iter()
                .map(|(movie_id, freqs)| {
                    let mut map = IndexMap::new();
                    for freq in freqs {
                        map.insert(
                            freq.lexeme.get_interned(&rodeo).unwrap(),
                            Frequency { count: freq.count },
                        );
                    }
                    (movie_id.clone(), map)
                })
                .collect()
        };

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

        Self {
            rodeo,
            translations,
            words_to_heteronyms,
            sentences_containing_lexeme_index,
            sentences_to_literals,
            sentences_to_lexemes,
            sentences_to_all_lexemes,
            word_frequencies,
            total_word_count,
            movie_word_frequencies,
            dictionary,
            phrasebook,
            word_to_pronunciation,
            pronunciation_to_words,
            pronunciation_data,
            pattern_frequency_map,
            homophone_practice,
            pronunciation_max_freq_cache,
            movies,
            sentence_sources,
            proper_noun_definitions,
        }
    }
}
