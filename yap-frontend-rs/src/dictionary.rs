use language_utils::features::{Morphology, WordPrefix};
use language_utils::text_cleanup::remove_accents_lowercase;
use language_utils::{Atom, Gram, GramDefinition, Language, TargetToNativeWord, WordType};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::{
    AudioRequest, CardData, CardIndicator, Deck, DeckEvent, LanguageEvent, LanguageEventContent,
};
use language_utils::{TtsProvider, TtsRequest};

/// Compute the grammatical prefix and morphology for a gram, given its definition
/// and target language.
///
/// For single-word grams (one `Atom::Tok`), uses the per-heteronym single-word
/// prefix — articles for nouns, subject pronouns for verbs, etc. — based on the
/// gram's heteronym and the first morphology entry from the dictionary definition.
/// Single-word prefixes only apply to Dictionary entries (Phrasebook entries lack
/// per-word morphology).
///
/// For multi-word grams (more than one `Atom::Tok`), uses the language-level
/// multiword prefix (e.g., Korean auxiliary connective endings on compound verbs).
/// Multi-word prefixes work for both Dictionary and Phrasebook entries since they
/// only depend on the gram's atom-level heteronym tags.
///
/// Morphology is only ever populated from Dictionary entries.
pub(crate) fn compute_word_prefix_and_morphology(
    gram: &Gram<String>,
    definition: &GramDefinition,
    target_language: Language,
) -> (Option<WordPrefix>, Option<Morphology>) {
    let dict_def = match definition {
        GramDefinition::Dictionary(d) => Some(d),
        GramDefinition::Phrasebook(_) => None,
    };

    match gram.atoms() {
        // Single-word gram: per-heteronym single-word prefix, only meaningful for
        // Dictionary entries (which carry per-word morphology).
        [Atom::Tok(word)] => {
            let WordType::Heteronym(h) = &word.word_type else {
                return (None, None);
            };
            let morphology = dict_def.and_then(|d| d.morphology.first().cloned());
            let prefix = morphology
                .as_ref()
                .and_then(|m| m.get_prefix(&h.word, h.pos, target_language));
            (prefix, morphology)
        }
        // Single-atom gram with no Tok: nothing to compute.
        [_] => (None, None),
        // No grams: nothing to compute
        [] => (None, None),
        // Multi-word gram: language-level multiword prefix. Dictionary entries
        // are always single-word, so there's no morphology to surface here.
        _ => (
            Morphology::get_multiword_prefix(gram, target_language),
            None,
        ),
    }
}

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
        let target_language = self.context.course.target_language;

        let query = search_query
            .filter(|q| !q.trim().is_empty())
            .map(|q| remove_accents_lowercase(&q));

        let mut entries: Vec<((u8, usize), GramDictionaryEntry)> = language_pack
            .gram_frequencies
            .entries
            .iter()
            .enumerate()
            .filter_map(|(frequency_index, (spur_gram, _freq))| {
                let gram_def = language_pack.gram_definitions.get(spur_gram)?;
                let resolved_gram = language_pack.resolve_gram(spur_gram);
                let display_text = resolved_gram.to_display_string(target_language);

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

                let entry = self.gram_dictionary_entry(frequency_index)?;
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

    /// Build the dictionary entry at a frequency index (None when out of
    /// range or the gram has no definition).
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn gram_dictionary_entry(&self, frequency_index: usize) -> Option<GramDictionaryEntry> {
        let language_pack = &self.context.language_pack;
        let target_language = self.context.course.target_language;
        let (spur_gram, _freq) = language_pack
            .gram_frequencies
            .entries
            .get_index(frequency_index)?;
        let gram_def = language_pack.gram_definitions.get(spur_gram)?;
        let resolved_gram = language_pack.resolve_gram(spur_gram);
        let display_text = resolved_gram.to_display_string(target_language);

        let card = CardIndicator::WrittenGram { gram: *spur_gram };
        let is_in_deck = matches!(self.cards.get(&card), Some(CardData::Added { .. }));

        let (prefix, morphology) =
            compute_word_prefix_and_morphology(&resolved_gram, gram_def, target_language);

        Some(match gram_def {
            GramDefinition::Dictionary(dict_def) => GramDictionaryEntry {
                display_text,
                frequency_index,
                is_in_deck,
                is_phrase: false,
                prefix,
                morphology,
                definition: GramDictionaryDefinition::Dictionary {
                    definitions: dict_def.definitions.clone(),
                },
                target_language,
            },
            GramDefinition::Phrasebook(pb_def) => GramDictionaryEntry {
                display_text,
                frequency_index,
                is_in_deck,
                is_phrase: true,
                prefix,
                morphology,
                definition: GramDictionaryDefinition::Phrasebook {
                    meaning: pb_def.meaning.clone(),
                    target_language_example: Some(pb_def.target_language_example.clone())
                        .filter(|s| !s.is_empty()),
                    native_language_example: Some(pb_def.native_language_example.clone())
                        .filter(|s| !s.is_empty()),
                },
                target_language,
            },
        })
    }

    /// Get the total number of gram dictionary entries (for "Showing X of Y" display)
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_gram_dictionary_count(&self) -> usize {
        let language_pack = &self.context.language_pack;
        language_pack
            .gram_frequencies
            .entries
            .iter()
            .filter(|(spur_gram, _)| language_pack.gram_definitions.contains_key(spur_gram))
            .count()
    }

    /// Create a DeckEvent for adding a gram/phrase by its frequency index
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn add_gram_by_frequency_index(&self, frequency_index: usize) -> Option<DeckEvent> {
        let language_pack = &self.context.language_pack;
        let (spur_gram, _freq) = language_pack
            .gram_frequencies
            .entries
            .get_index(frequency_index)?;

        let card = CardIndicator::WrittenGram { gram: *spur_gram };
        let resolved_card = card.resolve(&language_pack.string_rodeo, &language_pack.gram_rodeo);

        Some(DeckEvent::Language(LanguageEvent {
            target_language: self.context.course.target_language,
            native_language: self.context.course.native_language,
            content: LanguageEventContent::AddCards {
                cards: vec![resolved_card],
                sentence_list: None,
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
    target_language: Language,
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn audio_request(&self) -> AudioRequest {
        AudioRequest {
            request: TtsRequest {
                text: self.display_text.clone(),
                language: self.target_language,
                is_ssml: false,
                instructions: None,
                speed: 1.0,
            },
            provider: TtsProvider::Google,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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
