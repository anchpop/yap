//! V2 deck event types.

use crate::transcription_challenge;
use language_utils::{Heteronym, Language, Lexeme, PatternPosition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::hash::Hash;

#[derive(
    Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify, Hash,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub(super) enum CardIndicator<S> {
    TargetLanguage {
        lexeme: Lexeme<S>,
    },
    ListeningHomophonous {
        pronunciation: S,
    },
    ListeningHeteronym {
        heteronym: Heteronym<S>,
    },
    LetterPronunciation {
        pattern: S,
        position: PatternPosition,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub(super) enum SentenceReviewResult {
    Perfect {
        heteronyms_needed_hint: BTreeSet<Heteronym<String>>,
    },
    Wrong {
        submission: String,
        lexemes_remembered: BTreeSet<Lexeme<String>>,
        lexemes_forgotten: BTreeSet<Lexeme<String>>,
        heteronyms_needed_hint: BTreeSet<Heteronym<String>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub(super) enum SentenceReviewIndicator {
    TargetToNative {
        challenge_sentence: String,
        result: SentenceReviewResult,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(super) struct LanguageEvent {
    pub(super) target_language: Language,
    pub(super) native_language: Language,
    pub(super) content: LanguageEventContent,
}

#[derive(
    Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "lowercase")]
pub(super) enum Rating {
    Again,
    Remembered,
    Hard,
    Good,
    Easy,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(super) struct PlacementTest {
    pub(super) known_words: Vec<String>,
    pub(super) unknown_words: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub(super) enum LanguageEventContent {
    CompletePlacementTest {
        results: PlacementTest,
    },
    AddCards {
        cards: Vec<CardIndicator<String>>,
    },
    ReviewCard {
        reviewed: CardIndicator<String>,
        rating: Rating,
    },
    TranslationChallenge {
        review: SentenceReviewIndicator,
    },
    TranscriptionChallenge {
        challenge: Vec<transcription_challenge::PartGraded>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub(super) enum DeckEvent {
    Language(LanguageEvent),
}

/// Convert a V2 SentenceReviewIndicator to V3 SentenceReviewResult by looking up literals in context
/// Returns None if the sentence can't be found in the language pack
fn convert_sentence_review(
    review: SentenceReviewIndicator,
    context: &crate::Context,
) -> Option<super::current::SentenceReviewResult> {
    match review {
        SentenceReviewIndicator::TargetToNative {
            challenge_sentence,
            result,
        } => {
            // Clean the sentence to match language pack format (e.g., French punctuation spacing)
            let cleaned_sentence = language_utils::text_cleanup::cleanup_sentence(
                challenge_sentence.clone(),
                context.course.target_language,
            );

            // Look up the sentence in the language pack to get its literals
            let sentence_spur = context.language_pack.string_rodeo.get(&cleaned_sentence)?;
            let sentence_literals = context
                .language_pack
                .sentence_to_literals(&sentence_spur, context.course.target_language)?;

            match result {
                SentenceReviewResult::Perfect {
                    heteronyms_needed_hint,
                } => {
                    // Perfect = all remembered, just track which were hinted
                    let literals: Vec<_> = sentence_literals
                        .iter()
                        .map(|literal| {
                            let hinted = match &literal.word.word_type {
                                language_utils::WordType::Heteronym(h) => {
                                    Some(heteronyms_needed_hint.contains(h))
                                }
                                language_utils::WordType::Other(_) => None,
                            };
                            (literal.clone(), hinted)
                        })
                        .collect();

                    Some(super::current::SentenceReviewResult::Perfect {
                        challenge: challenge_sentence.clone(),
                        submission: challenge_sentence, // Use challenge as submission for perfect
                        literals,
                    })
                }
                SentenceReviewResult::Wrong {
                    submission,
                    lexemes_remembered,
                    lexemes_forgotten,
                    heteronyms_needed_hint,
                } => {
                    // Wrong = track remembered and hinted per literal
                    let literals: Vec<_> = sentence_literals
                        .iter()
                        .map(|literal| {
                            let result = match &literal.word.word_type {
                                language_utils::WordType::Heteronym(h) => {
                                    let lexeme = language_utils::Lexeme::Heteronym {
                                        heteronym: h.clone(),
                                    };
                                    let remembered = if lexemes_remembered.contains(&lexeme) {
                                        Some(true)
                                    } else if lexemes_forgotten.contains(&lexeme) {
                                        Some(false)
                                    } else {
                                        None
                                    };
                                    let hinted = heteronyms_needed_hint.contains(h);
                                    Some(super::current::LiteralResult { remembered, hinted })
                                }
                                language_utils::WordType::Other(_) => None,
                            };
                            (literal.clone(), result)
                        })
                        .collect();

                    // Get all phrases from the sentence and check their status
                    let phrases: Vec<_> = context
                        .language_pack
                        .encoded_sentences
                        .get(&sentence_spur)
                        .map(|encoded| {
                            encoded
                                .multiword_terms
                                .iter()
                                .chain(encoded.low_confidence_multiword_terms.iter())
                                .map(|phrase_gram| {
                                    let phrase = context
                                        .language_pack
                                        .gram_rodeo
                                        .resolve(phrase_gram)
                                        .resolve(&context.language_pack.string_rodeo)
                                        .to_display_string(context.course.target_language);
                                    let lexeme = language_utils::Lexeme::Multiword {
                                        phrase: phrase.clone(),
                                    };
                                    let remembered = if lexemes_remembered.contains(&lexeme) {
                                        Some(true)
                                    } else if lexemes_forgotten.contains(&lexeme) {
                                        Some(false)
                                    } else {
                                        None
                                    };
                                    (phrase, remembered)
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    Some(super::current::SentenceReviewResult::Graded {
                        challenge: challenge_sentence,
                        submission,
                        literals,
                        phrases,
                    })
                }
            }
        }
    }
}

impl DeckEvent {
    /// Migrate a V2 DeckEvent to V3 (current) format.
    pub(super) fn into_v3(self, context: &crate::Context) -> Option<super::current::DeckEvent> {
        match self {
            DeckEvent::Language(lang_event) => Some(super::current::DeckEvent::Language(
                super::current::LanguageEvent {
                    target_language: lang_event.target_language,
                    native_language: lang_event.native_language,
                    content: match lang_event.content {
                        LanguageEventContent::CompletePlacementTest { results } => {
                            super::current::LanguageEventContent::CompletePlacementTest {
                                results: super::current::PlacementTest {
                                    known_words: results.known_words,
                                    unknown_words: results.unknown_words,
                                },
                            }
                        }
                        LanguageEventContent::AddCards { cards } => {
                            super::current::LanguageEventContent::AddCards {
                                cards: cards
                                    .into_iter()
                                    .filter_map(|c| c.into_v3(context))
                                    .collect(),
                                sentence_list: None,
                            }
                        }
                        LanguageEventContent::ReviewCard { reviewed, rating } => {
                            let reviewed = reviewed.into_v3(context)?;
                            super::current::LanguageEventContent::ReviewCard {
                                reviewed,
                                rating: match rating {
                                    Rating::Again => super::current::Rating::Again,
                                    Rating::Remembered => super::current::Rating::Remembered,
                                    Rating::Hard => super::current::Rating::Hard,
                                    Rating::Good => super::current::Rating::Good,
                                    Rating::Easy => super::current::Rating::Easy,
                                },
                            }
                        }
                        LanguageEventContent::TranslationChallenge { review } => {
                            let legacy = match &review {
                                SentenceReviewIndicator::TargetToNative {
                                    result:
                                        SentenceReviewResult::Perfect {
                                            heteronyms_needed_hint,
                                        },
                                    ..
                                } => super::current::LegacyTranslationChallenge {
                                    lexemes_remembered: BTreeSet::new(),
                                    lexemes_forgotten: BTreeSet::new(),
                                    heteronyms_needed_hint: heteronyms_needed_hint.clone(),
                                },
                                SentenceReviewIndicator::TargetToNative {
                                    result:
                                        SentenceReviewResult::Wrong {
                                            lexemes_remembered,
                                            lexemes_forgotten,
                                            heteronyms_needed_hint,
                                            ..
                                        },
                                    ..
                                } => super::current::LegacyTranslationChallenge {
                                    lexemes_remembered: lexemes_remembered.clone(),
                                    lexemes_forgotten: lexemes_forgotten.clone(),
                                    heteronyms_needed_hint: heteronyms_needed_hint.clone(),
                                },
                            };
                            let sentence_review = convert_sentence_review(review, context)?;
                            super::current::LanguageEventContent::TranslationChallenge {
                                review: sentence_review,
                                legacy,
                            }
                        }
                        LanguageEventContent::TranscriptionChallenge { challenge } => {
                            super::current::LanguageEventContent::TranscriptionChallenge {
                                challenge,
                            }
                        }
                    },
                },
            )),
        }
    }
}

impl CardIndicator<String> {
    /// Convert V2 CardIndicator to V3 (current).
    /// Returns None if the card can't be converted (e.g., pronunciation/heteronym not found).
    pub(super) fn into_v3(
        self,
        context: &crate::Context,
    ) -> Option<super::current::CardIndicator<language_utils::Gram<String>, String>> {
        match self {
            CardIndicator::TargetLanguage { lexeme } => {
                match lexeme {
                    language_utils::Lexeme::Heteronym { heteronym } => {
                        // Convert heteronym to a single-word gram
                        let word = language_utils::Word {
                            text: heteronym.word.clone(),
                            word_type: language_utils::WordType::Heteronym(heteronym),
                        };
                        let atom = language_utils::Atom::Tok(word);
                        let gram = language_utils::Gram::new(vec![atom]);
                        Some(super::current::CardIndicator::WrittenGram { gram })
                    }
                    language_utils::Lexeme::Multiword { phrase } => {
                        // Find the gram that matches this phrase display string
                        let gram_spur = context
                            .language_pack
                            .string_to_grams
                            .get(&phrase)?
                            .first()?;
                        let gram = context
                            .language_pack
                            .gram_rodeo
                            .resolve(gram_spur)
                            .resolve(&context.language_pack.string_rodeo);
                        Some(super::current::CardIndicator::WrittenGram { gram })
                    }
                }
            }
            CardIndicator::ListeningHomophonous { pronunciation } => {
                // Convert pronunciation → heteronym → gram
                let pronunciation_spur = context.language_pack.string_rodeo.get(&pronunciation)?;
                // Get words with this pronunciation
                let words = context
                    .language_pack
                    .pronunciation_to_words
                    .get(&pronunciation_spur)?;
                // Get first heteronym from first word
                let word = words.first()?;
                let heteronyms = context.language_pack.words_to_heteronyms.get(word)?;
                let heteronym = heteronyms.first()?;
                // Look up the most common gram for this heteronym
                let gram_spur = context
                    .language_pack
                    .heteronym_to_grams
                    .get(heteronym)?
                    .first()?;
                // Drop low-frequency grams — the pronunciation-to-word mapping picks
                // the highest standalone-frequency word, which can be a rare conjugation
                // (e.g. "croient" over "crois") when common forms get absorbed into
                // multi-word grams. Threshold of 100 filters these out.
                let freq = context
                    .language_pack
                    .gram_frequencies
                    .entries
                    .get(gram_spur)?;
                if freq.count < 100 {
                    return None;
                }
                let gram = context
                    .language_pack
                    .gram_rodeo
                    .resolve(gram_spur)
                    .to_gram();
                let gram = gram.resolve(&context.language_pack.string_rodeo);
                Some(super::current::CardIndicator::ListeningGram { gram })
            }
            CardIndicator::ListeningHeteronym { heteronym } => {
                // Convert heteronym → gram using the index
                let interned_heteronym =
                    heteronym.get_interned(&context.language_pack.string_rodeo)?;
                let gram_spur = context
                    .language_pack
                    .heteronym_to_grams
                    .get(&interned_heteronym)?
                    .first()?;
                let gram = context
                    .language_pack
                    .gram_rodeo
                    .resolve(gram_spur)
                    .to_gram();
                let gram = gram.resolve(&context.language_pack.string_rodeo);
                Some(super::current::CardIndicator::ListeningGram { gram })
            }
            CardIndicator::LetterPronunciation { pattern, position } => {
                Some(super::current::CardIndicator::LetterPronunciation { pattern, position })
            }
        }
    }
}
