use language_utils::features::{Morphology, WordPrefix};
use language_utils::text_cleanup::remove_accents_lowercase;
use language_utils::{Atom, GramDefinition, TargetToNativeWord, WordType};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::{CardData, CardIndicator, Deck, DeckEvent, LanguageEvent, LanguageEventContent};

/// Get gram dictionary entries ordered by frequency (most common first).
/// Optionally filters by search query (accent-insensitive) and limits results.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Deck {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_gram_dictionary_entries(
        &self,
        search_query: Option<String>,
        limit: usize,
    ) -> Vec<GramDictionaryEntry> {
        let language_pack = &self.context.language_pack;
        let string_rodeo = &language_pack.string_rodeo;
        let gram_rodeo = &language_pack.gram_rodeo;
        let target_language = self.context.course.target_language;

        let query = search_query
            .filter(|q| !q.trim().is_empty())
            .map(|q| remove_accents_lowercase(&q));

        let mut entries: Vec<((u8, usize), GramDictionaryEntry)> = language_pack
            .gram_frequencies
            .iter()
            .enumerate()
            .filter_map(|(frequency_index, (spur_gram, _freq))| {
                let gram_def = language_pack.gram_definitions.get(spur_gram)?;
                let gram = gram_rodeo.resolve(spur_gram);
                let display_text = gram
                    .resolve(string_rodeo)
                    .to_display_string(target_language);

                // Filter by search query if provided, and compute relevance
                // 0 = exact match, 1 = starts with, 2 = contains
                let relevance = if let Some(q) = &query {
                    let normalized_display = remove_accents_lowercase(&display_text);
                    let display_exact = normalized_display == *q;
                    let display_starts = normalized_display.starts_with(q.as_str());
                    let display_contains = normalized_display.contains(q.as_str());
                    let definition_contains = match gram_def {
                        GramDefinition::Dictionary(dict_def) => dict_def
                            .definitions
                            .iter()
                            .any(|d| remove_accents_lowercase(&d.native).contains(q.as_str())),
                        GramDefinition::Phrasebook(pb_def) => {
                            remove_accents_lowercase(&pb_def.meaning).contains(q.as_str())
                        }
                    };
                    if !display_contains && !definition_contains {
                        return None;
                    }
                    if display_exact {
                        0
                    } else if display_starts {
                        1
                    } else {
                        2
                    }
                } else {
                    // No query — all entries have equal relevance
                    0
                };

                let card = CardIndicator::WrittenGram { gram: *spur_gram };
                let is_in_deck = matches!(self.cards.get(&card), Some(CardData::Added { .. }));

                let entry = match gram_def {
                    GramDefinition::Dictionary(dict_def) => {
                        // Extract heteronym from the single-atom gram for morphology/prefix lookup
                        let (prefix, morphology) = gram
                            .atoms()
                            .iter()
                            .find_map(|atom| {
                                if let Atom::Tok(word) = atom
                                    && let WordType::Heteronym(h) = &word.word_type
                                {
                                    let morph = dict_def.morphology.first().cloned();
                                    let word_text = string_rodeo.resolve(&h.word);
                                    let prefix = morph.as_ref().and_then(|m| {
                                        m.get_prefix(word_text, h.pos, target_language)
                                    });
                                    return Some((prefix, morph));
                                }
                                None
                            })
                            .unwrap_or((None, None));

                        GramDictionaryEntry {
                            display_text,
                            frequency_index,
                            is_in_deck,
                            is_phrase: false,
                            prefix,
                            morphology,
                            definition: GramDictionaryDefinition::Dictionary {
                                definitions: dict_def.definitions.clone(),
                            },
                        }
                    }
                    GramDefinition::Phrasebook(pb_def) => GramDictionaryEntry {
                        display_text,
                        frequency_index,
                        is_in_deck,
                        is_phrase: true,
                        prefix: None,
                        morphology: None,
                        definition: GramDictionaryDefinition::Phrasebook {
                            meaning: pb_def.meaning.clone(),
                            target_language_example: Some(pb_def.target_language_example.clone())
                                .filter(|s| !s.is_empty()),
                            native_language_example: Some(pb_def.native_language_example.clone())
                                .filter(|s| !s.is_empty()),
                        },
                    },
                };

                Some(((relevance, frequency_index), entry))
            })
            .collect();

        if entries.len() > limit {
            entries.select_nth_unstable_by_key(limit, |(key, _)| *key);
            entries.truncate(limit);
        }
        entries.sort_by_key(|(key, _)| *key);

        entries.into_iter().map(|(_, entry)| entry).collect()
    }

    /// Get the total number of gram dictionary entries (for "Showing X of Y" display)
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_gram_dictionary_count(&self) -> usize {
        let language_pack = &self.context.language_pack;
        language_pack
            .gram_frequencies
            .iter()
            .filter(|(spur_gram, _)| language_pack.gram_definitions.contains_key(spur_gram))
            .count()
    }

    /// Create a DeckEvent for adding a gram/phrase by its frequency index
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn add_gram_by_frequency_index(&self, frequency_index: usize) -> Option<DeckEvent> {
        let language_pack = &self.context.language_pack;
        let (spur_gram, _freq) = language_pack.gram_frequencies.get_index(frequency_index)?;

        let card = CardIndicator::WrittenGram { gram: *spur_gram };
        let resolved_card = card.resolve(&language_pack.string_rodeo, &language_pack.gram_rodeo);

        Some(DeckEvent::Language(LanguageEvent {
            target_language: self.context.course.target_language,
            native_language: self.context.course.native_language,
            content: LanguageEventContent::AddCards {
                cards: vec![resolved_card],
            },
        }))
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct GramDictionaryEntry {
    display_text: String,
    frequency_index: usize,
    is_in_deck: bool,
    is_phrase: bool,
    prefix: Option<WordPrefix>,
    morphology: Option<Morphology>,
    definition: GramDictionaryDefinition,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl GramDictionaryEntry {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn display_text(&self) -> String {
        self.display_text.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn frequency_index(&self) -> usize {
        self.frequency_index
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn is_in_deck(&self) -> bool {
        self.is_in_deck
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn is_phrase(&self) -> bool {
        self.is_phrase
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn prefix(&self) -> Option<WordPrefix> {
        self.prefix.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn morphology(&self) -> Option<Morphology> {
        self.morphology.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn definition(&self) -> GramDictionaryDefinition {
        self.definition.clone()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
pub enum GramDictionaryDefinition {
    Dictionary {
        definitions: Vec<TargetToNativeWord>,
    },
    Phrasebook {
        meaning: String,
        target_language_example: Option<String>,
        native_language_example: Option<String>,
    },
}
