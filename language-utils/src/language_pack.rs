use crate::indexmap::IndexMap;
use crate::minimal_pairs::{MinimalPairs, build_minimal_pairs_index};
use crate::{
    Atom, Audio, BookMetadata, ConsolidatedLanguageData, Course, DictionaryEntry, Frequency, Gram,
    GramDefinition, Heteronym, HomophonePractice, HomophoneWordPair, Lexeme, Literal, MorphemeInfo,
    MorphemeSegment, MovieMetadata, MultiwordTermMatch, PartOfSpeech, PatternPosition,
    PronunciationData, ProperNounDefinition, SentenceGram, SentenceGrams, SentenceSource, SpurGram,
    VoiceActor, WordType, grm,
};
use lasso::Spur;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

/// A frequency list with its total count (interned version for the language pack).
#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FrequencyList {
    pub entries: IndexMap<SpurGram, Frequency>,
    /// Total gram count from unfiltered data (for accurate percentage calculations)
    pub total_count: u64,
}

/// The assembled runtime pack. Never serialized itself — the on-disk/wire
/// format is the [`LanguagePackCore`] + [`LanguagePackSentences`] pair, put
/// back together by [`LanguagePack::from_parts`].
#[derive(Debug)]
pub struct LanguagePack {
    pub string_rodeo: lasso::RodeoReader,
    pub gram_rodeo: lasso::RodeoReader<Gram<Spur>>,
    pub translations: FxHashMap<Spur, Vec<Spur>>,
    pub words_to_heteronyms: FxHashMap<Spur, Vec<Heteronym<Spur>>>,
    /// Per-source gram frequencies (movies, Pimsleur lessons, etc.)
    pub source_gram_frequencies: FxHashMap<crate::FrequencySourceId, FrequencyList>,
    pub word_to_pronunciation: FxHashMap<Spur, Spur>,
    pub pronunciation_to_words: FxHashMap<Spur, Vec<Spur>>,
    /// Minimal-pair lookups: phoneme-pair → word pairs, and word → 1-off words.
    /// Computed from `word_to_pronunciation`; the IPA strings and differing
    /// position can be recovered on demand by diffing the two words'
    /// pronunciations.
    pub minimal_pairs: MinimalPairs,
    pub pronunciation_data: PronunciationData,
    pub pattern_frequency_map: FxHashMap<(Spur, PatternPosition), u32>,
    pub homophone_practice: FxHashMap<HomophoneWordPair<Spur>, HomophonePractice<Spur>>,
    /// Cache of maximum frequencies for each pronunciation (pre-computed at initialization)
    pub pronunciation_max_freq_cache: FxHashMap<Spur, Frequency>,
    /// Movie metadata indexed by movie ID
    pub movies: FxHashMap<String, MovieMetadata>,
    /// Book metadata indexed by book slug
    pub books: FxHashMap<String, BookMetadata>,
    /// Sentence source provenance tracking (maps sentence to its sources)
    pub sentence_sources: FxHashMap<Spur, SentenceSource>,
    /// Global proper noun definitions map
    pub proper_noun_definitions: BTreeMap<Spur, ProperNounDefinition>,
    /// Master gram frequencies
    pub gram_frequencies: FrequencyList,
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
    /// Morpheme classification + info, keyed by (surface, canonical) pair
    /// (both interned). Pair key prevents ambiguity when the same surface
    /// corresponds to different underlying morphemes.
    pub morphemes: FxHashMap<MorphemeSegment<Spur>, MorphemeInfo<Spur>>,
    /// Human-recorded audio clips, indexed by voice actor and then by the
    /// target-language phrase they speak.
    pub human_audio: FxHashMap<VoiceActor, FxHashMap<String, Audio>>,
}

impl LanguagePack {
    /// Intern a resolved gram back into this pack's rodeos. Returns None if
    /// any token, or the gram itself, was never interned.
    pub fn intern_gram(&self, gram: &Gram<String>) -> Option<SpurGram> {
        gram.get_interned(&self.string_rodeo)?
            .get_interned(&self.gram_rodeo)
    }

    /// Whether a gram is a real entry in this course. The rodeos intern more
    /// grams than the course actually teaches; the master frequency list is
    /// the canonical set.
    pub fn is_course_gram(&self, gram: &SpurGram) -> bool {
        self.gram_frequencies.entries.contains_key(gram)
    }

    /// Intern a gram and require it to be a real course entry.
    pub fn course_gram(&self, gram: &Gram<String>) -> Option<SpurGram> {
        self.intern_gram(gram).filter(|g| self.is_course_gram(g))
    }

    /// Resolve an interned gram back to its owned string form.
    pub fn resolve_gram(&self, gram: &SpurGram) -> Gram<String> {
        self.gram_rodeo.resolve(gram).resolve(&self.string_rodeo)
    }

    /// Sentences that contain `required_gram` (when given) and are otherwise fully
    /// comprehensible: every learnable gram and multiword term must satisfy
    /// `is_comprehensible` (the required gram itself always counts), and the
    /// sentence must have at least one translation.
    pub fn comprehensible_sentences(
        &self,
        required_gram: Option<&SpurGram>,
        is_comprehensible: impl Fn(&SpurGram) -> bool,
    ) -> Vec<Spur> {
        // Search through all sentences - if we have a required gram, only look at sentences containing it
        let candidate_sentences: Vec<Spur> = if let Some(required) = required_gram {
            match self.sentences_containing_gram_index.get(required) {
                Some(sentences) => sentences.clone(),
                None => return Vec::new(),
            }
        } else {
            // If no required gram/phrase, consider all sentences
            self.translations.keys().cloned().collect()
        };

        let is_comprehensible = |gram: &SpurGram| {
            is_comprehensible(gram) || required_gram.is_some_and(|req| req == gram)
        };

        let mut possible_sentences = Vec::new();

        // Warning: this loop is HOT!
        'checkSentences: for sentence in candidate_sentences {
            let Some(sentence_grams) = self.encoded_sentences.get(&sentence) else {
                continue;
            };

            // Check that all learnable grams are comprehensible
            for sentence_gram in &sentence_grams.grams {
                if let SentenceGram::Learnable(gram) = sentence_gram
                    && !is_comprehensible(gram)
                {
                    continue 'checkSentences; // Early exit!
                }
            }

            // Check that all multiword terms (high and low confidence) are comprehensible
            for multiword_gram in sentence_grams
                .multiword_terms
                .iter()
                .chain(sentence_grams.low_confidence_multiword_terms.iter())
            {
                if !is_comprehensible(&multiword_gram.gram) {
                    continue 'checkSentences; // Early exit!
                }
            }

            // Check that the sentence has at least one translation
            if self
                .translations
                .get(&sentence)
                .is_none_or(|t| t.is_empty())
            {
                continue 'checkSentences;
            }

            possible_sentences.push(sentence);
        }

        possible_sentences
    }

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

    /// Native-language gloss for a single heteronym: finds its most-frequent
    /// gram, looks up the dictionary entry, and returns the first definition.
    pub fn heteronym_gloss(&self, heteronym: &Heteronym<Spur>) -> Option<String> {
        let gram = self.heteronym_to_grams.get(heteronym)?.first()?;
        match self.gram_definitions.get(gram)? {
            GramDefinition::Dictionary(d) => d.definitions.first().map(|td| td.native.clone()),
            GramDefinition::Phrasebook(_) => None,
        }
    }

    /// Get a learner-facing gloss for a morpheme.
    ///
    /// - `Root` morphemes are glossed by looking up the heteronym's first
    ///   dictionary entry and joining its native-language definitions.
    /// - `Bound`, `Derivation`, and `Inflection` return their stored `meaning`.
    ///
    /// Returns `None` if no gloss is available (e.g. a `Root` whose heteronym
    /// has no dictionary entry, or an affix with `meaning: None`).
    pub fn morpheme_gloss(&self, info: &MorphemeInfo<Spur>) -> Option<String> {
        match info {
            MorphemeInfo::Root { heteronym } => self.heteronym_gloss(heteronym),
            MorphemeInfo::Bound { meaning }
            | MorphemeInfo::Derivation { meaning }
            | MorphemeInfo::Inflection { meaning } => {
                meaning.map(|spur| self.string_rodeo.resolve(&spur).to_string())
            }
        }
    }

    /// Break a heteronym's word into its constituent morphemes. Each entry is
    /// `(surface, canonical, gloss)`. The gloss is required for a morpheme-level
    /// breakdown (all-or-nothing); `canonical` is `Some` only when it differs
    /// from the surface (and from the full word).
    fn compute_morpheme_breakdown(
        &self,
        heteronym: &Heteronym<Spur>,
    ) -> Option<Vec<(String, Option<String>, String)>> {
        let rodeo = &self.string_rodeo;

        // The segmentation lives on the DictionaryEntry, reached via the
        // heteronym's first (most frequent) gram.
        let gram = self.heteronym_to_grams.get(heteronym)?.first()?;
        let dict_entry = match self.gram_definitions.get(gram)? {
            GramDefinition::Dictionary(d) => d,
            GramDefinition::Phrasebook(_) => return None,
        };

        if dict_entry.segments.len() < 2 {
            return None;
        }

        let full_word = rodeo.resolve(&heteronym.word);

        dict_entry
            .segments
            .iter()
            .map(|segment| {
                let key = MorphemeSegment {
                    surface: rodeo.get(&segment.surface)?,
                    canonical: rodeo.get(&segment.canonical)?,
                    tag: match &segment.tag {
                        Some(t) => Some(rodeo.get(t)?),
                        None => None,
                    },
                };
                let info = self.morphemes.get(&key)?;
                let gloss = self.morpheme_gloss(info)?;
                let canonical =
                    if segment.canonical == segment.surface || segment.canonical == full_word {
                        None
                    } else {
                        Some(segment.canonical.clone())
                    };
                Some((segment.surface.clone(), canonical, gloss))
            })
            .collect()
    }

    /// Learner-facing breakdown of a gram. Each entry is
    /// `(surface, canonical, gloss)`; `canonical` is `Some` only when it differs
    /// from the surface, and `gloss` is optional so punctuation atoms in
    /// multi-word grams can render with an empty native-language cell.
    ///
    /// - Single-heteronym grams → **morpheme-level** decomposition (all glosses
    ///   required, all-or-nothing).
    /// - Multi-atom grams → **word-level** decomposition, but only when the
    ///   phrase has a `Phrasebook` entry flagged `compositional` (for idiomatic
    ///   phrases, per-word glosses are misleading).
    #[allow(clippy::type_complexity)]
    pub fn compute_breakdown(
        &self,
        gram_spur: SpurGram,
    ) -> Option<Vec<(String, Option<String>, Option<String>)>> {
        let gram = self.gram_rodeo.resolve(&gram_spur);
        match gram.atoms() {
            [Atom::Tok(word)] => match &word.word_type {
                WordType::Heteronym(het) => {
                    let inner = self.compute_morpheme_breakdown(het)?;
                    Some(inner.into_iter().map(|(s, c, g)| (s, c, Some(g))).collect())
                }
                _ => None,
            },
            atoms => {
                let is_compositional = matches!(
                    self.gram_definitions.get(&gram_spur),
                    Some(GramDefinition::Phrasebook(entry)) if entry.compositional
                );
                if !is_compositional {
                    return None;
                }
                let rodeo = &self.string_rodeo;
                let out: Vec<(String, Option<String>, Option<String>)> = atoms
                    .iter()
                    .filter_map(|atom| match atom {
                        Atom::Tok(word) => {
                            let surface = rodeo.resolve(&word.text).to_string();
                            let (canonical, gloss) = match &word.word_type {
                                WordType::Heteronym(het) => {
                                    let lemma = rodeo.resolve(&het.lemma);
                                    let canonical = if lemma == surface {
                                        None
                                    } else {
                                        Some(lemma.to_string())
                                    };
                                    (canonical, self.heteronym_gloss(het))
                                }
                                _ => (None, None),
                            };
                            Some((surface, canonical, gloss))
                        }
                        _ => None,
                    })
                    .collect();
                if out.len() < 2 { None } else { Some(out) }
            }
        }
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

            for entry in &language_data.gram_frequencies.entries {
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
                    hets.sort_by_key(|b| std::cmp::Reverse(b.1));
                    (word, hets.into_iter().map(|(h, _)| h).collect::<Vec<_>>())
                })
                .collect()
        };

        let word_to_pronunciation = {
            language_data
                .word_to_pronunciation
                .iter()
                .map(|(word, pronunciations)| {
                    (
                        rodeo
                            .get(word)
                            .unwrap_or_else(|| panic!("word not in rodeo: {word:?}")),
                        // Language pack only stores the main pronunciation;
                        // verifier alternates aren't surfaced to the frontend yet.
                        rodeo.get(&pronunciations.main).unwrap_or_else(|| {
                            panic!("pronunciation not in rodeo: {:?}", pronunciations.main)
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

        // Initialize movie and book data
        let movies = language_data.movies;
        let books = language_data.books;

        let human_audio = language_data.human_audio.clone();

        // Store source_gram_frequencies to convert after gram_rodeo is created
        let source_gram_frequencies_data = language_data.source_gram_frequencies.clone();

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
                let interned_multiword_terms: Option<Vec<MultiwordTermMatch<SpurGram>>> = encoded
                    .multiword_terms
                    .iter()
                    .map(|term| {
                        let interned = term.gram.get_interned(&rodeo)?;
                        Some(MultiwordTermMatch {
                            gram: interned.get_interned(&gram_rodeo)?,
                            matched_word_indices: term.matched_word_indices.clone(),
                        })
                    })
                    .collect();
                let interned_low_confidence_multiword_terms: Option<
                    Vec<MultiwordTermMatch<SpurGram>>,
                > = encoded
                    .low_confidence_multiword_terms
                    .iter()
                    .map(|term| {
                        let interned = term.gram.get_interned(&rodeo)?;
                        Some(MultiwordTermMatch {
                            gram: interned.get_interned(&gram_rodeo)?,
                            matched_word_indices: term.matched_word_indices.clone(),
                        })
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
            for entry in &language_data.gram_frequencies.entries {
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
        let gram_frequencies_map: IndexMap<SpurGram, Frequency> = {
            const COGNATE_BONUS: f32 = 2.0;

            // First pass: compute base ease for single-atom grams so we can reference them
            // when computing multi-atom gram ease.
            let mut single_atom_ease: FxHashMap<Atom<Spur>, f32> = FxHashMap::default();
            for (gram_spur, (count, _direct_count)) in gram_counts.iter() {
                let gram = gram_rodeo.resolve(gram_spur);
                if gram.len() == 1
                    && let Some(atom) = gram.iter().next()
                {
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
                    // A learnable component with no standalone gram never occurs outside this
                    // gram, so it can't be easier than the gram itself — fall back to ln_count
                    // instead of skipping it. Skipping would let the easy components set the
                    // ease (e.g. "est infecté" inheriting the ease of "est" when "infecté" has
                    // no entry of its own).
                    let min_component_ease = gram
                        .iter()
                        .filter_map(|atom| {
                            let ease = single_atom_ease.get(atom).copied();
                            match atom {
                                Atom::Tok(word)
                                    if matches!(word.word_type, WordType::Heteronym(_)) =>
                                {
                                    Some(ease.unwrap_or(ln_count))
                                }
                                _ => ease,
                            }
                        })
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

        let gram_frequencies = FrequencyList {
            total_count: language_data.gram_frequencies.total_count,
            entries: gram_frequencies_map,
        };

        // Build index from heteronym to all grams composed only of that heteronym, sorted by frequency
        let heteronym_to_grams: FxHashMap<Heteronym<Spur>, Vec<SpurGram>> = {
            let mut map: FxHashMap<Heteronym<Spur>, Vec<(SpurGram, Frequency)>> =
                FxHashMap::default();
            for (gram_spur, freq) in gram_frequencies.entries.iter() {
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
                    v.sort_by_key(|b| std::cmp::Reverse(b.1.count));
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
                for term in &sentence_grams.multiword_terms {
                    map.entry(term.gram).or_default().push(*sentence_spur);
                }
                // Add low-confidence multiword term grams to index
                for term in &sentence_grams.low_confidence_multiword_terms {
                    map.entry(term.gram).or_default().push(*sentence_spur);
                }
            }
            map
        };

        // Convert per-source gram frequencies (pre-computed in generate-data)
        // Each entry has a `gram` field (Gram<String>) which we intern
        let source_gram_frequencies: FxHashMap<crate::FrequencySourceId, FrequencyList> =
            source_gram_frequencies_data
                .iter()
                .map(|(source_id, freq_list)| {
                    let mut map: IndexMap<SpurGram, Frequency> = IndexMap::new();
                    for entry in &freq_list.entries {
                        let interned_gram = entry.gram.get_interned(&rodeo);
                        if let Some(interned_gram) = interned_gram
                            && let Some(gram_spur) = gram_rodeo.get(&interned_gram)
                        {
                            // Use ease from global gram_frequencies; fall back to ln(count)
                            let global_freq = gram_frequencies.entries.get(&gram_spur);
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
                    (
                        source_id.clone(),
                        FrequencyList {
                            entries: map,
                            total_count: freq_list.total_count,
                        },
                    )
                })
                .filter(|(_, list)| !list.entries.is_empty())
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
                            .filter_map(|gram_spur| {
                                gram_frequencies.entries.get(gram_spur).copied()
                            })
                            .max_by_key(|f| f.count)
                    })
                    .max_by_key(|f| f.count)?;
                Some((*pronunciation, max_freq))
            })
            .collect();

        // For each word, the max frequency of any single-atom gram whose only
        // atom is that word — used to sort minimal-pair neighbor lists so the
        // most common standalone-usage neighbors appear first.
        // `heteronym_to_grams` is already pre-filtered to single-atom heteronym
        // grams (its construction at line ~536 enforces that) and sorted by
        // frequency desc, so the top gram's frequency for each heteronym is
        // the per-heteronym answer; we take the max across the word's
        // heteronyms.
        let word_max_single_gram_freq: FxHashMap<Spur, u32> = words_to_heteronyms
            .iter()
            .filter_map(|(word, hets)| {
                let max = hets
                    .iter()
                    .filter_map(|het| {
                        let top_gram = heteronym_to_grams.get(het)?.first()?;
                        gram_frequencies.entries.get(top_gram).map(|f| f.count)
                    })
                    .max()?;
                Some((*word, max))
            })
            .collect();

        let minimal_pairs = build_minimal_pairs_index(
            &language_data.minimal_pairs,
            &rodeo,
            &word_max_single_gram_freq,
        );

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

        let morphemes: FxHashMap<MorphemeSegment<Spur>, MorphemeInfo<Spur>> = language_data
            .morphemes
            .iter()
            .map(|(segment, info)| {
                let key = MorphemeSegment {
                    surface: rodeo.get(&segment.surface).unwrap(),
                    canonical: rodeo.get(&segment.canonical).unwrap(),
                    tag: segment.tag.as_ref().map(|t| rodeo.get(t).unwrap()),
                };
                let interned = match info {
                    MorphemeInfo::Root { heteronym } => MorphemeInfo::Root {
                        heteronym: Heteronym {
                            word: rodeo.get(&heteronym.word).unwrap(),
                            lemma: rodeo.get(&heteronym.lemma).unwrap(),
                            pos: heteronym.pos,
                        },
                    },
                    MorphemeInfo::Bound { meaning } => MorphemeInfo::Bound {
                        meaning: meaning.as_ref().map(|m| rodeo.get(m).unwrap()),
                    },
                    MorphemeInfo::Derivation { meaning } => MorphemeInfo::Derivation {
                        meaning: meaning.as_ref().map(|m| rodeo.get(m).unwrap()),
                    },
                    MorphemeInfo::Inflection { meaning } => MorphemeInfo::Inflection {
                        meaning: meaning.as_ref().map(|m| rodeo.get(m).unwrap()),
                    },
                };
                (key, interned)
            })
            .collect();

        Self {
            string_rodeo: rodeo,
            gram_rodeo,
            translations,
            words_to_heteronyms,
            source_gram_frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
            minimal_pairs,
            pronunciation_data,
            pattern_frequency_map,
            homophone_practice,
            pronunciation_max_freq_cache,
            movies,
            books,
            sentence_sources,
            proper_noun_definitions,
            gram_frequencies,
            encoded_sentences,
            gram_definitions,
            heteronym_to_grams,
            sentences_containing_gram_index,
            string_to_grams,
            morphemes,
            human_audio,
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

/// The word-level half of a language pack: dictionary, frequency,
/// pronunciation and morphology data. Small enough (~5x smaller than the
/// sentence half) to download first, so the placement test can run before
/// the sentence data has arrived.
///
/// The two halves share one spur space: `string_rodeo` holds strings `0..N`
/// and `gram_rodeo` grams `0..G`; [`LanguagePackSentences`] carries the
/// extensions (`N..M` strings, `G..H` grams). A core and a sentences part
/// only fit together when they come from the same [`LanguagePack::split`]
/// call — the pair must be versioned and shipped as one unit.
#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LanguagePackCore {
    /// Fingerprint of this core's spur space (see [`spur_space_fingerprint`]).
    /// [`LanguagePack::from_parts`] refuses a sentences half whose
    /// fingerprint differs, so halves from different builds can't be
    /// silently combined.
    pub spur_space_fingerprint: u64,
    pub string_rodeo: lasso::RodeoReader,
    pub gram_rodeo: lasso::RodeoReader<Gram<Spur>>,
    pub words_to_heteronyms: FxHashMap<Spur, Vec<Heteronym<Spur>>>,
    pub word_to_pronunciation: FxHashMap<Spur, Spur>,
    pub pronunciation_to_words: FxHashMap<Spur, Vec<Spur>>,
    pub minimal_pairs: MinimalPairs,
    pub pronunciation_data: PronunciationData,
    pub pattern_frequency_map: FxHashMap<(Spur, PatternPosition), u32>,
    pub pronunciation_max_freq_cache: FxHashMap<Spur, Frequency>,
    pub proper_noun_definitions: BTreeMap<Spur, ProperNounDefinition>,
    pub gram_frequencies: FrequencyList,
    pub gram_definitions: FxHashMap<SpurGram, GramDefinition>,
    pub heteronym_to_grams: FxHashMap<Heteronym<Spur>, Vec<SpurGram>>,
    pub string_to_grams: FxHashMap<String, Vec<SpurGram>>,
    pub morphemes: FxHashMap<MorphemeSegment<Spur>, MorphemeInfo<Spur>>,
}

/// The sentence half of a language pack: sentence texts, translations,
/// per-sentence gram encodings and indexes, source metadata and human audio.
/// See [`LanguagePackCore`] for the spur-space contract between the halves.
#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LanguagePackSentences {
    /// Fingerprint of the core spur space this half was built against.
    pub spur_space_fingerprint: u64,
    /// Strings `N..M` of the shared spur space, in spur order.
    pub string_extension: Vec<String>,
    /// Grams `G..H` of the shared spur space, in spur order. Their atoms may
    /// reference extension strings.
    pub gram_extension: Vec<Gram<Spur>>,
    pub translations: FxHashMap<Spur, Vec<Spur>>,
    pub source_gram_frequencies: FxHashMap<crate::FrequencySourceId, FrequencyList>,
    pub homophone_practice: FxHashMap<HomophoneWordPair<Spur>, HomophonePractice<Spur>>,
    pub movies: FxHashMap<String, MovieMetadata>,
    pub books: FxHashMap<String, BookMetadata>,
    pub sentence_sources: FxHashMap<Spur, SentenceSource>,
    pub encoded_sentences: FxHashMap<Spur, SentenceGrams<SpurGram>>,
    pub sentences_containing_gram_index: FxHashMap<SpurGram, Vec<Spur>>,
    pub human_audio: FxHashMap<VoiceActor, FxHashMap<String, Audio>>,
}

/// Fingerprint of a core spur space: every core string and every core gram,
/// in spur order. A sentences half fits a core exactly when the core's spur
/// space (both interners' contents *and* key assignment) matches the one it
/// was split against, which is precisely what this hashes — identical strings
/// alone are not enough, since e.g. a reordered frequency list reassigns gram
/// spurs without changing any string. Identical content keeps producing
/// identical fingerprints across rebuilds (the archives stay reproducible).
fn spur_space_fingerprint(core_strings: &[String], core_grams: &[Gram<Spur>]) -> u64 {
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    hasher.update(&(core_strings.len() as u64).to_le_bytes());
    hasher.update(&(core_grams.len() as u64).to_le_bytes());
    for s in core_strings {
        hasher.update(&(s.len() as u64).to_le_bytes());
        hasher.update(s.as_bytes());
    }
    for g in core_grams {
        // A gram is atoms over the (already-hashed) string spur space; its
        // rkyv encoding is a stable byte form of exactly that.
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(g)
            .expect("gram serialization for fingerprint cannot fail");
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    hasher.digest()
}

/// Re-interns spurs from an existing pack's rodeos into a fresh, ordered
/// spur space. Interning order is what establishes the core/extension
/// partition: everything interned before the cutoff snapshot lands in the
/// core, everything after in the extension.
struct SpurRemapper<'a> {
    old_strings: &'a lasso::RodeoReader,
    old_grams: &'a lasso::RodeoReader<Gram<Spur>>,
    strings: lasso::Rodeo,
    grams: lasso::Rodeo<Gram<Spur>>,
}

impl SpurRemapper<'_> {
    fn s(&mut self, spur: Spur) -> Spur {
        self.strings.get_or_intern(self.old_strings.resolve(&spur))
    }

    fn het(&mut self, h: &Heteronym<Spur>) -> Heteronym<Spur> {
        Heteronym {
            word: self.s(h.word),
            lemma: self.s(h.lemma),
            pos: h.pos,
        }
    }

    fn atom(&mut self, a: &Atom<Spur>) -> Atom<Spur> {
        match a {
            Atom::Tok(word) => Atom::Tok(crate::Word {
                text: self.s(word.text),
                word_type: match &word.word_type {
                    WordType::Heteronym(h) => WordType::Heteronym(self.het(h)),
                    WordType::Other(o) => WordType::Other(*o),
                },
            }),
            Atom::Control(c) => Atom::Control(*c),
        }
    }

    fn g(&mut self, g: SpurGram) -> SpurGram {
        let atoms: Vec<Atom<Spur>> = self
            .old_grams
            .resolve(&g)
            .atoms()
            .iter()
            .map(|a| self.atom(a))
            .collect();
        self.grams.get_or_intern(Gram(atoms))
    }

    fn seg(&mut self, s: &MorphemeSegment<Spur>) -> MorphemeSegment<Spur> {
        MorphemeSegment {
            surface: self.s(s.surface),
            canonical: self.s(s.canonical),
            tag: s.tag.map(|t| self.s(t)),
        }
    }
}

/// Sort a hash map's entries by key so remapping (and therefore spur
/// assignment) is deterministic across builds.
fn sorted<K: Ord, V>(map: FxHashMap<K, V>) -> Vec<(K, V)> {
    let mut entries: Vec<(K, V)> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

impl LanguagePack {
    /// Split this pack into its core (word-level) and sentence halves,
    /// re-interning everything into a fresh spur space where all core
    /// strings/grams come before all sentence-side ones. The partition is
    /// established purely by processing order, so a future field added to
    /// the pack just needs to be handled on the correct side of the cutoff —
    /// the exhaustive destructure below makes forgetting one a compile error.
    pub fn split(self) -> (LanguagePackCore, LanguagePackSentences) {
        let LanguagePack {
            string_rodeo,
            gram_rodeo,
            translations,
            words_to_heteronyms,
            source_gram_frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
            minimal_pairs,
            pronunciation_data,
            pattern_frequency_map,
            homophone_practice,
            pronunciation_max_freq_cache,
            movies,
            books,
            sentence_sources,
            proper_noun_definitions,
            gram_frequencies,
            encoded_sentences,
            gram_definitions,
            heteronym_to_grams,
            sentences_containing_gram_index,
            string_to_grams,
            morphemes,
            human_audio,
        } = self;

        let mut r = SpurRemapper {
            old_strings: &string_rodeo,
            old_grams: &gram_rodeo,
            strings: lasso::Rodeo::new(),
            grams: lasso::Rodeo::new(),
        };

        // ---- Core phase: everything interned here lands below the cutoff. ----

        // Frequencies first so the most common words get the densest spurs.
        let gram_frequencies = FrequencyList {
            entries: gram_frequencies
                .entries
                .iter()
                .map(|(g, freq)| (r.g(*g), *freq))
                .collect(),
            total_count: gram_frequencies.total_count,
        };

        let gram_definitions: FxHashMap<SpurGram, GramDefinition> = sorted(gram_definitions)
            .into_iter()
            .map(|(g, def)| (r.g(g), def))
            .collect();

        let words_to_heteronyms: FxHashMap<Spur, Vec<Heteronym<Spur>>> =
            sorted(words_to_heteronyms)
                .into_iter()
                .map(|(w, hets)| (r.s(w), hets.iter().map(|h| r.het(h)).collect()))
                .collect();

        let word_to_pronunciation: FxHashMap<Spur, Spur> = sorted(word_to_pronunciation)
            .into_iter()
            .map(|(w, p)| (r.s(w), r.s(p)))
            .collect();

        let pronunciation_to_words: FxHashMap<Spur, Vec<Spur>> = sorted(pronunciation_to_words)
            .into_iter()
            .map(|(p, ws)| (r.s(p), ws.into_iter().map(|w| r.s(w)).collect()))
            .collect();

        let minimal_pairs = MinimalPairs {
            by_phoneme_pair: sorted(minimal_pairs.by_phoneme_pair)
                .into_iter()
                .map(|((a, b), pairs)| {
                    (
                        (r.s(a), r.s(b)),
                        pairs.into_iter().map(|(x, y)| (r.s(x), r.s(y))).collect(),
                    )
                })
                .collect(),
            by_word: sorted(minimal_pairs.by_word)
                .into_iter()
                .map(|(w, ns)| (r.s(w), ns.into_iter().map(|n| r.s(n)).collect()))
                .collect(),
        };

        let pattern_frequency_map: FxHashMap<(Spur, PatternPosition), u32> =
            sorted(pattern_frequency_map)
                .into_iter()
                .map(|((p, pos), f)| ((r.s(p), pos), f))
                .collect();

        let pronunciation_max_freq_cache: FxHashMap<Spur, Frequency> =
            sorted(pronunciation_max_freq_cache)
                .into_iter()
                .map(|(p, f)| (r.s(p), f))
                .collect();

        let proper_noun_definitions: BTreeMap<Spur, ProperNounDefinition> = proper_noun_definitions
            .into_iter()
            .map(|(n, d)| (r.s(n), d))
            .collect();

        let heteronym_to_grams: FxHashMap<Heteronym<Spur>, Vec<SpurGram>> =
            sorted(heteronym_to_grams)
                .into_iter()
                .map(|(h, gs)| (r.het(&h), gs.into_iter().map(|g| r.g(g)).collect()))
                .collect();

        let string_to_grams: FxHashMap<String, Vec<SpurGram>> = sorted(string_to_grams)
            .into_iter()
            .map(|(s, gs)| (s, gs.into_iter().map(|g| r.g(g)).collect()))
            .collect();

        let morphemes: FxHashMap<MorphemeSegment<Spur>, MorphemeInfo<Spur>> = sorted(morphemes)
            .into_iter()
            .map(|(seg, info)| {
                let info = match info {
                    MorphemeInfo::Root { heteronym } => MorphemeInfo::Root {
                        heteronym: r.het(&heteronym),
                    },
                    MorphemeInfo::Bound { meaning } => MorphemeInfo::Bound {
                        meaning: meaning.map(|m| r.s(m)),
                    },
                    MorphemeInfo::Derivation { meaning } => MorphemeInfo::Derivation {
                        meaning: meaning.map(|m| r.s(m)),
                    },
                    MorphemeInfo::Inflection { meaning } => MorphemeInfo::Inflection {
                        meaning: meaning.map(|m| r.s(m)),
                    },
                };
                (r.seg(&seg), info)
            })
            .collect();

        // ---- Cutoff: spurs below these counts are the core's. ----
        let core_string_count = r.strings.len();
        let core_gram_count = r.grams.len();

        // ---- Sentence phase: everything from here lands in the extension. ----

        let encoded_sentences: FxHashMap<Spur, SentenceGrams<SpurGram>> = sorted(encoded_sentences)
            .into_iter()
            .map(|(sentence, sg)| {
                (
                    r.s(sentence),
                    SentenceGrams {
                        grams: sg
                            .grams
                            .into_iter()
                            .map(|g| match g {
                                SentenceGram::Learnable(g) => SentenceGram::Learnable(r.g(g)),
                                SentenceGram::Obvious(g) => SentenceGram::Obvious(r.g(g)),
                            })
                            .collect(),
                        capitalize_first: sg.capitalize_first,
                        multiword_terms: sg
                            .multiword_terms
                            .into_iter()
                            .map(|t| MultiwordTermMatch {
                                gram: r.g(t.gram),
                                matched_word_indices: t.matched_word_indices,
                            })
                            .collect(),
                        low_confidence_multiword_terms: sg
                            .low_confidence_multiword_terms
                            .into_iter()
                            .map(|t| MultiwordTermMatch {
                                gram: r.g(t.gram),
                                matched_word_indices: t.matched_word_indices,
                            })
                            .collect(),
                    },
                )
            })
            .collect();

        let translations: FxHashMap<Spur, Vec<Spur>> = sorted(translations)
            .into_iter()
            .map(|(s, ts)| (r.s(s), ts.into_iter().map(|t| r.s(t)).collect()))
            .collect();

        let sentences_containing_gram_index: FxHashMap<SpurGram, Vec<Spur>> =
            sorted(sentences_containing_gram_index)
                .into_iter()
                .map(|(g, ss)| (r.g(g), ss.into_iter().map(|s| r.s(s)).collect()))
                .collect();

        let sentence_sources: FxHashMap<Spur, SentenceSource> = sorted(sentence_sources)
            .into_iter()
            .map(|(s, src)| (r.s(s), src))
            .collect();

        let source_gram_frequencies: FxHashMap<crate::FrequencySourceId, FrequencyList> =
            sorted(source_gram_frequencies)
                .into_iter()
                .map(|(id, list)| {
                    (
                        id,
                        FrequencyList {
                            entries: list.entries.iter().map(|(g, f)| (r.g(*g), *f)).collect(),
                            total_count: list.total_count,
                        },
                    )
                })
                .collect();

        let homophone_practice: FxHashMap<HomophoneWordPair<Spur>, HomophonePractice<Spur>> =
            sorted(homophone_practice)
                .into_iter()
                .map(|(pair, practice)| {
                    (
                        HomophoneWordPair {
                            word1: r.s(pair.word1),
                            word2: r.s(pair.word2),
                        },
                        HomophonePractice {
                            sentence_pairs: practice
                                .sentence_pairs
                                .into_iter()
                                .map(|p| crate::HomophoneSentencePair {
                                    sentence1: r.s(p.sentence1),
                                    sentence2: r.s(p.sentence2),
                                })
                                .collect(),
                        },
                    )
                })
                .collect();

        // Sweep everything else the old rodeos held (strings/grams referenced
        // by no field) into the extension, so the assembled pack's rodeos
        // resolve exactly the same set as the original.
        for i in 0..string_rodeo.len() {
            let spur = <Spur as lasso::Key>::try_from_usize(i).unwrap();
            r.s(spur);
        }
        for i in 0..gram_rodeo.len() {
            let spur = <SpurGram as lasso::Key>::try_from_usize(i).unwrap();
            r.g(spur);
        }

        let SpurRemapper { strings, grams, .. } = r;
        let full_strings = strings.into_reader();
        let full_grams = grams.into_reader();

        let resolve_range_strings = |from: usize, to: usize| -> Vec<String> {
            (from..to)
                .map(|i| {
                    full_strings
                        .resolve(&<Spur as lasso::Key>::try_from_usize(i).unwrap())
                        .to_owned()
                })
                .collect()
        };
        let resolve_range_grams = |from: usize, to: usize| -> Vec<Gram<Spur>> {
            (from..to)
                .map(|i| {
                    Gram(
                        full_grams
                            .resolve(&<SpurGram as lasso::Key>::try_from_usize(i).unwrap())
                            .atoms()
                            .to_vec(),
                    )
                })
                .collect()
        };

        // The core ships rodeos holding only the below-cutoff entries.
        let core_strings = resolve_range_strings(0, core_string_count);
        let core_grams = resolve_range_grams(0, core_gram_count);
        let fingerprint = spur_space_fingerprint(&core_strings, &core_grams);
        let core_string_rodeo = {
            let mut rodeo = lasso::Rodeo::new();
            for s in core_strings {
                rodeo.get_or_intern(s);
            }
            rodeo.into_reader()
        };
        let core_gram_rodeo = {
            let mut rodeo: lasso::Rodeo<Gram<Spur>> = lasso::Rodeo::new();
            for g in core_grams {
                rodeo.get_or_intern(g);
            }
            rodeo.into_reader()
        };

        let string_extension = resolve_range_strings(core_string_count, full_strings.len());
        let gram_extension = resolve_range_grams(core_gram_count, full_grams.len());

        (
            LanguagePackCore {
                spur_space_fingerprint: fingerprint,
                string_rodeo: core_string_rodeo,
                gram_rodeo: core_gram_rodeo,
                words_to_heteronyms,
                word_to_pronunciation,
                pronunciation_to_words,
                minimal_pairs,
                pronunciation_data,
                pattern_frequency_map,
                pronunciation_max_freq_cache,
                proper_noun_definitions,
                gram_frequencies,
                gram_definitions,
                heteronym_to_grams,
                string_to_grams,
                morphemes,
            },
            LanguagePackSentences {
                spur_space_fingerprint: fingerprint,
                string_extension,
                gram_extension,
                translations,
                source_gram_frequencies,
                homophone_practice,
                movies,
                books,
                sentence_sources,
                encoded_sentences,
                sentences_containing_gram_index,
                human_audio,
            },
        )
    }

    /// Assemble a runtime pack from its halves. With `sentences: None`, the
    /// result has empty sentence-side maps — enough for the placement test
    /// and dictionary lookups, but no comprehensible-sentence search.
    pub fn from_parts(core: LanguagePackCore, sentences: Option<LanguagePackSentences>) -> Self {
        let LanguagePackCore {
            spur_space_fingerprint,
            string_rodeo,
            gram_rodeo,
            words_to_heteronyms,
            word_to_pronunciation,
            pronunciation_to_words,
            minimal_pairs,
            pronunciation_data,
            pattern_frequency_map,
            pronunciation_max_freq_cache,
            proper_noun_definitions,
            gram_frequencies,
            gram_definitions,
            heteronym_to_grams,
            string_to_grams,
            morphemes,
        } = core;

        if let Some(sentences) = &sentences {
            assert_eq!(
                spur_space_fingerprint, sentences.spur_space_fingerprint,
                "language pack halves are from different builds (core spur-space fingerprint \
                 doesn't match the one the sentences half was split against)"
            );
        }

        let Some(sentences) = sentences else {
            return LanguagePack {
                string_rodeo,
                gram_rodeo,
                translations: FxHashMap::default(),
                words_to_heteronyms,
                source_gram_frequencies: FxHashMap::default(),
                word_to_pronunciation,
                pronunciation_to_words,
                minimal_pairs,
                pronunciation_data,
                pattern_frequency_map,
                homophone_practice: FxHashMap::default(),
                pronunciation_max_freq_cache,
                movies: FxHashMap::default(),
                books: FxHashMap::default(),
                sentence_sources: FxHashMap::default(),
                proper_noun_definitions,
                gram_frequencies,
                encoded_sentences: FxHashMap::default(),
                gram_definitions,
                heteronym_to_grams,
                sentences_containing_gram_index: FxHashMap::default(),
                string_to_grams,
                morphemes,
                human_audio: FxHashMap::default(),
            };
        };

        // Rebuild the full rodeos: core entries keep their spurs (lasso
        // assigns keys sequentially in interning order), extension entries
        // take the spurs after the cutoff. The length asserts catch a
        // mismatched core/sentences pair.
        let core_string_count = string_rodeo.len();
        let string_rodeo = {
            let mut rodeo = lasso::Rodeo::new();
            for i in 0..core_string_count {
                let spur = <Spur as lasso::Key>::try_from_usize(i).unwrap();
                let interned = rodeo.get_or_intern(string_rodeo.resolve(&spur));
                debug_assert_eq!(interned, spur);
            }
            for s in &sentences.string_extension {
                rodeo.get_or_intern(s.as_str());
            }
            assert_eq!(
                rodeo.len(),
                core_string_count + sentences.string_extension.len(),
                "language pack core/sentences string spaces overlap — mismatched halves?"
            );
            rodeo.into_reader()
        };

        let core_gram_count = gram_rodeo.len();
        let gram_rodeo = {
            let mut rodeo: lasso::Rodeo<Gram<Spur>> = lasso::Rodeo::new();
            for i in 0..core_gram_count {
                let spur = <SpurGram as lasso::Key>::try_from_usize(i).unwrap();
                let gram = Gram(gram_rodeo.resolve(&spur).atoms().to_vec());
                let interned = rodeo.get_or_intern(gram);
                debug_assert_eq!(interned, spur);
            }
            for g in &sentences.gram_extension {
                rodeo.get_or_intern(g.clone());
            }
            assert_eq!(
                rodeo.len(),
                core_gram_count + sentences.gram_extension.len(),
                "language pack core/sentences gram spaces overlap — mismatched halves?"
            );
            rodeo.into_reader()
        };

        LanguagePack {
            string_rodeo,
            gram_rodeo,
            translations: sentences.translations,
            words_to_heteronyms,
            source_gram_frequencies: sentences.source_gram_frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
            minimal_pairs,
            pronunciation_data,
            pattern_frequency_map,
            homophone_practice: sentences.homophone_practice,
            pronunciation_max_freq_cache,
            movies: sentences.movies,
            books: sentences.books,
            sentence_sources: sentences.sentence_sources,
            proper_noun_definitions,
            gram_frequencies,
            encoded_sentences: sentences.encoded_sentences,
            gram_definitions,
            heteronym_to_grams,
            sentences_containing_gram_index: sentences.sentences_containing_gram_index,
            string_to_grams,
            morphemes,
            human_audio: sentences.human_audio,
        }
    }
}

/// Filenames of the two halves of a split language pack, relative to a
/// course's data directory. Shared by the generator, the converters, the
/// backend and every native loader so they can't drift apart.
pub const CORE_FILENAME: &str = "language_data_core.rkyv";
pub const SENTENCES_FILENAME: &str = "language_data_sentences.rkyv";

/// Load and assemble a full language pack from a course directory holding
/// the split `language_data_core.rkyv` + `language_data_sentences.rkyv` pair.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_split_dir(dir: &std::path::Path) -> Result<LanguagePack, std::io::Error> {
    let read = |filename: &str| {
        std::fs::read(dir.join(filename)).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!("failed to read {filename} in {}: {e}", dir.display()),
            )
        })
    };
    let rkyv_err = |e: rkyv::rancor::Error| std::io::Error::other(e.to_string());
    let core_bytes = read(CORE_FILENAME)?;
    let sentences_bytes = read(SENTENCES_FILENAME)?;
    let core = rkyv::access::<ArchivedLanguagePackCore, rkyv::rancor::Error>(&core_bytes)
        .map_err(rkyv_err)?;
    let core =
        rkyv::deserialize::<LanguagePackCore, rkyv::rancor::Error>(core).map_err(rkyv_err)?;
    let sentences =
        rkyv::access::<ArchivedLanguagePackSentences, rkyv::rancor::Error>(&sentences_bytes)
            .map_err(rkyv_err)?;
    let sentences = rkyv::deserialize::<LanguagePackSentences, rkyv::rancor::Error>(sentences)
        .map_err(rkyv_err)?;
    Ok(LanguagePack::from_parts(core, Some(sentences)))
}

/// Split `pack` and write the pair plus the `language_data.hash` metadata
/// file (line 1: core `hash;size`, line 2: sentences `hash;size`) into a
/// course directory. The one writer shared by the generator and the
/// converter, so the on-disk layout has a single source of truth.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_split_dir(dir: &std::path::Path, pack: LanguagePack) -> Result<(), std::io::Error> {
    use xxhash_rust::const_xxh3::xxh3_64;

    let (core, sentences) = pack.split();
    let core_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&core)
        .expect("failed to serialize language pack core");
    let sentences_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&sentences)
        .expect("failed to serialize language pack sentences");

    std::fs::write(dir.join(CORE_FILENAME), &core_bytes)?;
    std::fs::write(dir.join(SENTENCES_FILENAME), &sentences_bytes)?;
    std::fs::write(
        dir.join("language_data.hash"),
        format!(
            "{};{}\n{};{}",
            xxh3_64(&core_bytes),
            core_bytes.len(),
            xxh3_64(&sentences_bytes),
            sentences_bytes.len()
        ),
    )?;
    Ok(())
}
