//! Current (V3) deck event types.

use std::collections::BTreeSet;

use crate::deck_selection::DailyReviewTarget;
use crate::transcription_challenge;
use crate::{CardType, FlashcardType};
use language_utils::{Gram, Language, Literal, PatternPosition, SpurGram};
use lasso::Spur;
use serde::{Deserialize, Serialize};

#[bridgerton::bridge(transparent)]
#[derive(
    Copy,
    Clone,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    Hash,
    schemars::JsonSchema,
)]
#[serde(tag = "type")]
// Every variant is an object; saying so at the top level keeps MCP clients
// that key off `type` from treating the union as untyped.
#[schemars(extend("type" = "object"))]
pub enum CardIndicator<G, S> {
    WrittenGram {
        gram: G,
    },
    ListeningGram {
        gram: G,
    },
    LetterPronunciation {
        pattern: S,
        position: PatternPosition,
    },
}

impl<G, S> CardIndicator<G, S> {
    pub fn written_gram(&self) -> Option<&G> {
        match self {
            CardIndicator::WrittenGram { gram } => Some(gram),
            _ => None,
        }
    }

    pub fn listening_gram(&self) -> Option<&G> {
        match self {
            CardIndicator::ListeningGram { gram } => Some(gram),
            _ => None,
        }
    }

    pub fn letter_pronunciation(&self) -> Option<&S> {
        match self {
            CardIndicator::LetterPronunciation { pattern, .. } => Some(pattern),
            _ => None,
        }
    }

    pub fn card_type(&self) -> CardType {
        match self {
            CardIndicator::WrittenGram { .. } => CardType::TargetLanguage,
            CardIndicator::ListeningGram { .. } => CardType::Listening,
            CardIndicator::LetterPronunciation { .. } => CardType::LetterPronunciation,
        }
    }
}

impl CardIndicator<Gram<String>, String> {
    pub fn get_interned(
        &self,
        string_rodeo: &lasso::RodeoReader,
        gram_rodeo: &lasso::RodeoReader<Gram<Spur>>,
    ) -> Option<CardIndicator<SpurGram, Spur>> {
        Some(match self {
            CardIndicator::WrittenGram { gram } => {
                let gram = gram.get_interned(string_rodeo)?.get_interned(gram_rodeo)?;
                CardIndicator::WrittenGram { gram }
            }
            CardIndicator::ListeningGram { gram } => {
                let gram = gram.get_interned(string_rodeo)?.get_interned(gram_rodeo)?;
                CardIndicator::ListeningGram { gram }
            }
            CardIndicator::LetterPronunciation { pattern, position } => {
                CardIndicator::LetterPronunciation {
                    pattern: string_rodeo.get(pattern)?,
                    position: *position,
                }
            }
        })
    }
}

impl CardIndicator<SpurGram, Spur> {
    pub fn get_flashcard_type(&self) -> Option<FlashcardType> {
        match self {
            CardIndicator::WrittenGram { .. } => Some(FlashcardType::WrittenGram),
            CardIndicator::ListeningGram { .. } => Some(FlashcardType::Listening),
            CardIndicator::LetterPronunciation { .. } => Some(FlashcardType::LetterPronunciation),
        }
    }

    pub fn resolve(
        &self,
        string_rodeo: &lasso::RodeoReader,
        gram_rodeo: &lasso::RodeoReader<Gram<Spur>>,
    ) -> CardIndicator<Gram<String>, String> {
        match self {
            CardIndicator::WrittenGram { gram } => {
                let gram_atoms = gram_rodeo.resolve(gram);
                CardIndicator::WrittenGram {
                    gram: gram_atoms.resolve(string_rodeo),
                }
            }
            CardIndicator::ListeningGram { gram } => {
                let gram_atoms = gram_rodeo.resolve(gram);
                CardIndicator::ListeningGram {
                    gram: gram_atoms.resolve(string_rodeo),
                }
            }
            CardIndicator::LetterPronunciation { pattern, position } => {
                CardIndicator::LetterPronunciation {
                    pattern: string_rodeo.resolve(pattern).to_string(),
                    position: *position,
                }
            }
        }
    }
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub struct LiteralResult {
    pub remembered: Option<bool>,
    pub hinted: bool,
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(tag = "type")]
pub enum GradePart<S> {
    Ungradable,
    Graded { val: S },
}

/// Alias for backwards compatibility with v2 migration code
pub type TranscriptionGradePart<S> = GradePart<S>;

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(tag = "type")]
pub enum SentenceReviewResult {
    Perfect {
        challenge: String,
        submission: String,
        /// None for Other word types, Some(hinted) for heteronyms
        literals: Vec<(Literal<String>, Option</* hinted */ bool>)>,
    },
    Graded {
        challenge: String,
        submission: String,
        /// None for Other word types, Some(result) for heteronyms
        literals: Vec<(Literal<String>, Option<LiteralResult>)>,
        phrases: Vec<(String, /* remembered */ Option<bool>)>,
    },
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub struct LanguageEvent {
    pub target_language: Language,
    pub native_language: Language,
    pub content: LanguageEventContent,
}

#[bridgerton::bridge(transparent)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    Again,
    Remembered,
    Hard,
    Good,
    Easy,
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub struct PlacementTest {
    pub known_words: Vec<String>,
    pub unknown_words: Vec<String>,
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(tag = "type")]
pub enum SentenceListSelection {
    Movie {
        id: String,
    },
    PimsleurLesson {
        level: u32,
        #[serde(alias = "unit")]
        lesson: u32,
    },
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(tag = "type")]
pub enum LanguageEventContent {
    CompletePlacementTest {
        results: PlacementTest,
    },
    AddCards {
        cards: Vec<CardIndicator<Gram<String>, String>>,
        #[serde(default, alias = "goal")]
        sentence_list: Option<SentenceListSelection>,
    },
    ReviewCard {
        reviewed: CardIndicator<Gram<String>, String>,
        rating: Rating,
    },
    TranslationChallenge {
        review: SentenceReviewResult,
        #[serde(skip)]
        legacy: LegacyTranslationChallenge,
    },
    TranscriptionChallenge {
        challenge: Vec<transcription_challenge::PartGraded>,
    },
    #[serde(alias = "SetGoal")]
    SetSentenceList {
        #[serde(alias = "goal")]
        sentence_list: Option<SentenceListSelection>,
    },
    SetDailyReviewTarget {
        daily_review_target: DailyReviewTarget,
    },
    /// Lock up every added card EXCEPT the listed ones. Locked cards are
    /// hidden from the review queue (and only from the review queue) until
    /// released via `UnlockCards` or until any review touches them.
    #[serde(alias = "LockDueCardsExcept")]
    LockCardsExcept {
        keep: Vec<CardIndicator<Gram<String>, String>>,
    },
    /// Release the listed cards from lockup.
    UnlockCards {
        cards: Vec<CardIndicator<Gram<String>, String>>,
    },
}

#[derive(PartialEq, Eq, Ord, PartialOrd, Default, Clone, Debug)]
pub struct LegacyTranslationChallenge {
    pub lexemes_remembered: BTreeSet<language_utils::Lexeme<String>>,
    pub lexemes_forgotten: BTreeSet<language_utils::Lexeme<String>>,
    pub heteronyms_needed_hint: BTreeSet<language_utils::Heteronym<String>>,
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(tag = "type")]
pub enum DeckEvent {
    Language(LanguageEvent),
}
