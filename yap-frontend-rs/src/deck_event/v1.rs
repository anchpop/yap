//! V1 deck event types.

use language_utils::{Language, PatternPosition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::hash::Hash;

// V1-local Lexeme types (frozen for deserialization compatibility)

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum PartOfSpeech {
    #[serde(rename = "ADJ")]
    Adj,
    #[serde(rename = "ADP")]
    Adp,
    #[serde(rename = "ADV")]
    Adv,
    #[serde(rename = "AUX")]
    Aux,
    #[serde(rename = "CCONJ")]
    Cconj,
    #[serde(rename = "DET")]
    Det,
    #[serde(rename = "INTJ")]
    Intj,
    #[serde(rename = "NOUN")]
    Noun,
    #[serde(rename = "NUM")]
    Num,
    #[serde(rename = "PART")]
    Part,
    #[serde(rename = "PRON")]
    Pron,
    #[serde(rename = "PROPN")]
    Propn,
    #[serde(rename = "PUNCT")]
    Punct,
    #[serde(rename = "SCONJ")]
    Sconj,
    #[serde(rename = "SYM")]
    Sym,
    #[serde(rename = "VERB")]
    Verb,
    #[serde(rename = "SPACE")]
    Space,
    #[serde(rename = "X")]
    X,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum OtherWordType {
    #[serde(rename = "PROPN")]
    Propn,
    #[serde(rename = "PUNCT")]
    Punct,
    #[serde(rename = "SPACE")]
    Space,
    #[serde(rename = "X")]
    X,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct OtherWord {
    pub(super) other_tag: OtherWordType,
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(untagged)]
pub(super) enum WordType<S> {
    Heteronym(Heteronym<S>),
    Other(OtherWord),
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Word<S> {
    pub(super) text: S,
    pub(super) word_type: WordType<S>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Literal<S> {
    pub(super) word: Word<S>,
    pub(super) whitespace: S,
}

impl<S> serde::Serialize for Literal<S>
where
    S: serde::Serialize,
{
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Literal", 3)?;
        state.serialize_field("text", &self.word.text)?;
        state.serialize_field("word_type", &self.word.word_type)?;
        state.serialize_field("whitespace", &self.whitespace)?;
        state.end()
    }
}

impl<'de, S> serde::Deserialize<'de> for Literal<S>
where
    S: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;
        use std::marker::PhantomData;

        struct LiteralVisitor<S>(PhantomData<S>);

        impl<'de, S> Visitor<'de> for LiteralVisitor<S>
        where
            S: serde::Deserialize<'de>,
        {
            type Value = Literal<S>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Literal")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Literal<S>, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut text: Option<S> = None;
                let mut word_type: Option<WordType<S>> = None;
                let mut whitespace: Option<S> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "text" => {
                            if text.is_some() {
                                return Err(de::Error::duplicate_field("text"));
                            }
                            text = Some(map.next_value()?);
                        }
                        "word_type" => {
                            if word_type.is_some() {
                                return Err(de::Error::duplicate_field("word_type"));
                            }
                            word_type = Some(map.next_value()?);
                        }
                        "heteronym" => {
                            // Legacy format: heteronym is Option<Heteronym<S>>
                            if word_type.is_some() {
                                return Err(de::Error::duplicate_field("word_type"));
                            }
                            let heteronym: Option<Heteronym<S>> = map.next_value()?;
                            word_type = Some(match heteronym {
                                Some(h) => WordType::Heteronym(h),
                                None => WordType::Other(OtherWord {
                                    other_tag: OtherWordType::X,
                                }),
                            });
                        }
                        "whitespace" => {
                            if whitespace.is_some() {
                                return Err(de::Error::duplicate_field("whitespace"));
                            }
                            whitespace = Some(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let text = text.ok_or_else(|| de::Error::missing_field("text"))?;
                let word_type = word_type.ok_or_else(|| de::Error::missing_field("word_type"))?;
                let whitespace =
                    whitespace.ok_or_else(|| de::Error::missing_field("whitespace"))?;

                Ok(Literal {
                    word: Word { text, word_type },
                    whitespace,
                })
            }
        }

        deserializer.deserialize_map(LiteralVisitor(PhantomData))
    }
}

impl Literal<String> {
    /// Convert v1 Literal to current (language_utils) Literal.
    pub(super) fn to_v2(self) -> language_utils::Literal<String> {
        language_utils::Literal {
            word: language_utils::Word {
                text: self.word.text,
                word_type: self.word.word_type.to_v2(),
            },
            whitespace: self.whitespace,
        }
    }
}

impl WordType<String> {
    /// Convert v1 WordType to current (language_utils) WordType.
    pub(super) fn to_v2(self) -> language_utils::WordType<String> {
        match self {
            WordType::Heteronym(h) => h.pos.to_word_type(h.word, h.lemma),
            WordType::Other(o) => language_utils::WordType::Other(language_utils::OtherWord {
                other_tag: match o.other_tag {
                    OtherWordType::Propn => language_utils::OtherWordType::Propn,
                    OtherWordType::Punct => language_utils::OtherWordType::Punct,
                    OtherWordType::Space => language_utils::OtherWordType::Space,
                    OtherWordType::X => language_utils::OtherWordType::X,
                },
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Heteronym<S> {
    pub(super) word: S,
    pub(super) lemma: S,
    pub(super) pos: PartOfSpeech,
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Lexeme<S> {
    Heteronym(Heteronym<S>),
    Multiword(S),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub(super) enum CardIndicator<S> {
    TargetLanguage {
        lexeme: Lexeme<S>,
    },
    ListeningHomophonous {
        pronunciation: S,
    },
    ListeningLexeme {
        lexeme: Lexeme<S>,
    },
    LetterPronunciation {
        pattern: S,
        position: PatternPosition,
    },
}

impl CardIndicator<String> {
    pub(super) fn to_v2(self) -> Option<super::v2::CardIndicator<String>> {
        Some(match self {
            CardIndicator::TargetLanguage { lexeme } => super::v2::CardIndicator::TargetLanguage {
                lexeme: lexeme.to_lexeme()?,
            },
            CardIndicator::ListeningHomophonous { pronunciation } => {
                super::v2::CardIndicator::ListeningHomophonous { pronunciation }
            }
            CardIndicator::ListeningLexeme { lexeme } => {
                // Only Heteronym lexemes can become ListeningHeteronym cards
                let heteronym = match lexeme {
                    Lexeme::Heteronym(h) => {
                        let pos = h.pos.to_heteronym_pos()?;
                        language_utils::Heteronym {
                            word: h.word,
                            lemma: h.lemma,
                            pos,
                        }
                    }
                    Lexeme::Multiword(_) => return None, // Multiwords can't be ListeningHeteronym
                };
                super::v2::CardIndicator::ListeningHeteronym { heteronym }
            }
            CardIndicator::LetterPronunciation { pattern, position } => {
                super::v2::CardIndicator::LetterPronunciation { pattern, position }
            }
        })
    }
}

impl Lexeme<String> {
    /// Convert to the current WordType representation.
    /// Returns Heteronym for valid heteronym POS, Other for non-heteronym POS.
    pub(super) fn to_word_type(self) -> language_utils::WordType<String> {
        match self {
            Lexeme::Heteronym(h) => h.pos.to_word_type(h.word, h.lemma),
            Lexeme::Multiword(s) => {
                // Multiword -> treat as a noun heteronym
                language_utils::WordType::Heteronym(language_utils::Heteronym {
                    word: s.clone(),
                    lemma: s,
                    pos: language_utils::PartOfSpeech::Noun,
                })
            }
        }
    }

    /// Try to convert to a current Lexeme. Returns None if conversion results in non-heteronym.
    pub(super) fn to_lexeme(self) -> Option<language_utils::Lexeme<String>> {
        match self {
            Lexeme::Heteronym(h) => {
                let pos = h.pos.to_heteronym_pos()?;
                Some(language_utils::Lexeme::Heteronym {
                    heteronym: language_utils::Heteronym {
                        word: h.word,
                        lemma: h.lemma,
                        pos,
                    },
                })
            }
            Lexeme::Multiword(s) => Some(language_utils::Lexeme::Multiword { phrase: s }),
        }
    }
}

impl PartOfSpeech {
    /// Convert to the current WordType representation.
    pub(super) fn to_word_type(
        self,
        word: String,
        lemma: String,
    ) -> language_utils::WordType<String> {
        match self.to_heteronym_pos() {
            Some(pos) => {
                language_utils::WordType::Heteronym(language_utils::Heteronym { word, lemma, pos })
            }
            None => language_utils::WordType::Other(language_utils::OtherWord {
                other_tag: match self {
                    PartOfSpeech::Propn => language_utils::OtherWordType::Propn,
                    PartOfSpeech::Punct => language_utils::OtherWordType::Punct,
                    PartOfSpeech::Space => language_utils::OtherWordType::Space,
                    PartOfSpeech::X => language_utils::OtherWordType::X,
                    _ => unreachable!(),
                },
            }),
        }
    }

    /// Convert to language_utils::PartOfSpeech if this is a valid heteronym POS.
    /// Returns None for non-heteronym types (Propn, Punct, Space, X).
    pub(super) fn to_heteronym_pos(self) -> Option<language_utils::PartOfSpeech> {
        Some(match self {
            PartOfSpeech::Adj => language_utils::PartOfSpeech::Adj,
            PartOfSpeech::Adp => language_utils::PartOfSpeech::Adp,
            PartOfSpeech::Adv => language_utils::PartOfSpeech::Adv,
            PartOfSpeech::Aux => language_utils::PartOfSpeech::Aux,
            PartOfSpeech::Cconj => language_utils::PartOfSpeech::Cconj,
            PartOfSpeech::Det => language_utils::PartOfSpeech::Det,
            PartOfSpeech::Intj => language_utils::PartOfSpeech::Intj,
            PartOfSpeech::Noun => language_utils::PartOfSpeech::Noun,
            PartOfSpeech::Num => language_utils::PartOfSpeech::Num,
            PartOfSpeech::Part => language_utils::PartOfSpeech::Part,
            PartOfSpeech::Pron => language_utils::PartOfSpeech::Pron,
            PartOfSpeech::Sconj => language_utils::PartOfSpeech::Sconj,
            PartOfSpeech::Sym => language_utils::PartOfSpeech::Sym,
            PartOfSpeech::Verb => language_utils::PartOfSpeech::Verb,
            // Non-heteronym types
            PartOfSpeech::Propn | PartOfSpeech::Punct | PartOfSpeech::Space | PartOfSpeech::X => {
                return None;
            }
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub(super) enum SentenceReviewResult {
    Perfect {
        #[serde(default)]
        lexemes_needed_hint: BTreeSet<Lexeme<String>>,
    },
    Wrong {
        submission: String,
        lexemes_remembered: BTreeSet<Lexeme<String>>,
        lexemes_forgotten: BTreeSet<Lexeme<String>>,
        #[serde(default)]
        lexemes_needed_hint: BTreeSet<Lexeme<String>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub(super) enum SentenceReviewIndicator {
    TargetToNative {
        challenge_sentence: String,
        result: SentenceReviewResult,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub(super) struct LanguageEvent {
    #[serde(alias = "language")]
    pub(super) target_language: Language,
    #[serde(default = "default_native_language")]
    pub(super) native_language: Language,
    pub(super) content: LanguageEventContent,
}

fn default_native_language() -> Language {
    Language::English
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub(super) enum Rating {
    Again,
    Remembered, // generic rating for when the user picked "remembered" without choosing a specific rating

    Hard,
    Good,
    Easy,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub(super) struct PlacementTest {
    pub(super) known_words: Vec<String>,
    pub(super) unknown_words: Vec<String>,
}

// V1 transcription challenge types

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub(super) enum WordGrade {
    Perfect {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wrote: Option<String>,
    },
    CorrectWithTypo {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wrote: Option<String>,
    },
    PhoneticallyIdenticalButContextuallyIncorrect {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wrote: Option<String>,
    },
    PhoneticallySimilarButContextuallyIncorrect {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wrote: Option<String>,
    },
    Incorrect {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wrote: Option<String>,
    },
    Missed {},
}

impl WordGrade {
    pub(super) fn to_v2(self) -> language_utils::transcription_challenge::WordGrade {
        use language_utils::transcription_challenge::WordGrade as Current;
        match self {
            WordGrade::Perfect { wrote } => Current::Perfect { wrote },
            WordGrade::CorrectWithTypo { wrote } => Current::CorrectWithTypo { wrote },
            WordGrade::PhoneticallyIdenticalButContextuallyIncorrect { wrote } => {
                Current::PhoneticallyIdenticalButContextuallyIncorrect { wrote }
            }
            WordGrade::PhoneticallySimilarButContextuallyIncorrect { wrote } => {
                Current::PhoneticallySimilarButContextuallyIncorrect { wrote }
            }
            WordGrade::Incorrect { wrote } => Current::Incorrect { wrote },
            WordGrade::Missed {} => Current::Missed {},
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub(super) struct PartGradedPart {
    pub(super) heard: Literal<String>,
    pub(super) grade: WordGrade,
}

impl PartGradedPart {
    pub(super) fn to_v2(self) -> language_utils::transcription_challenge::PartGradedPart {
        language_utils::transcription_challenge::PartGradedPart {
            heard: self.heard.to_v2(),
            grade: self.grade.to_v2(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub(super) enum PartGraded {
    AskedToTranscribe {
        parts: Vec<PartGradedPart>,
        submission: String,
    },
    Provided {
        part: Literal<String>,
    },
}

impl PartGraded {
    pub(super) fn to_v2(self) -> language_utils::transcription_challenge::PartGraded {
        use language_utils::transcription_challenge::PartGraded as Current;
        match self {
            PartGraded::AskedToTranscribe { parts, submission } => Current::AskedToTranscribe {
                parts: parts.into_iter().map(|p| p.to_v2()).collect(),
                submission,
            },
            PartGraded::Provided { part } => Current::Provided { part: part.to_v2() },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
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
    #[serde(rename = "ReviewSentence")]
    TranslationChallenge {
        review: SentenceReviewIndicator,
    },
    TranscriptionChallenge {
        challenge: Vec<PartGraded>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub(super) enum DeckEvent {
    Language(LanguageEvent),
}

impl DeckEvent {
    /// Migrate a V1 DeckEvent to V2 format.
    /// Returns None if the event cannot be converted (e.g., card type no longer exists).
    pub(super) fn to_v2(self) -> Option<super::v2::DeckEvent> {
        match self {
            DeckEvent::Language(lang_event) => {
                let content = match lang_event.content {
                    LanguageEventContent::CompletePlacementTest { results } => {
                        super::v2::LanguageEventContent::CompletePlacementTest {
                            results: super::v2::PlacementTest {
                                known_words: results.known_words,
                                unknown_words: results.unknown_words,
                            },
                        }
                    }
                    LanguageEventContent::AddCards { cards } => {
                        super::v2::LanguageEventContent::AddCards {
                            cards: cards.into_iter().filter_map(|c| c.to_v2()).collect(),
                        }
                    }
                    LanguageEventContent::ReviewCard { reviewed, rating } => {
                        // If card can't be converted, skip this entire event
                        let reviewed = reviewed.to_v2()?;
                        super::v2::LanguageEventContent::ReviewCard {
                            reviewed,
                            rating: match rating {
                                Rating::Again => super::v2::Rating::Again,
                                Rating::Remembered => super::v2::Rating::Remembered,
                                Rating::Hard => super::v2::Rating::Hard,
                                Rating::Good => super::v2::Rating::Good,
                                Rating::Easy => super::v2::Rating::Easy,
                            },
                        }
                    }
                    LanguageEventContent::TranslationChallenge { review } => {
                        super::v2::LanguageEventContent::TranslationChallenge {
                            review: match review {
                                SentenceReviewIndicator::TargetToNative {
                                    challenge_sentence,
                                    result,
                                } => super::v2::SentenceReviewIndicator::TargetToNative {
                                    challenge_sentence,
                                    result: match result {
                                        SentenceReviewResult::Perfect {
                                            lexemes_needed_hint,
                                        } => super::v2::SentenceReviewResult::Perfect {
                                            heteronyms_needed_hint: lexemes_needed_hint
                                                .into_iter()
                                                .filter_map(|l| l.to_lexeme())
                                                .flat_map(|l| l.heteronym().cloned())
                                                .collect(),
                                        },
                                        SentenceReviewResult::Wrong {
                                            submission,
                                            lexemes_remembered,
                                            lexemes_forgotten,
                                            lexemes_needed_hint,
                                        } => super::v2::SentenceReviewResult::Wrong {
                                            submission,
                                            lexemes_remembered: lexemes_remembered
                                                .into_iter()
                                                .filter_map(|l| l.to_lexeme())
                                                .collect(),
                                            lexemes_forgotten: lexemes_forgotten
                                                .into_iter()
                                                .filter_map(|l| l.to_lexeme())
                                                .collect(),
                                            heteronyms_needed_hint: lexemes_needed_hint
                                                .into_iter()
                                                .filter_map(|l| l.to_lexeme())
                                                .flat_map(|l| l.heteronym().cloned())
                                                .collect(),
                                        },
                                    },
                                },
                            },
                        }
                    }
                    LanguageEventContent::TranscriptionChallenge { challenge } => {
                        super::v2::LanguageEventContent::TranscriptionChallenge {
                            challenge: challenge.into_iter().map(|p| p.to_v2()).collect(),
                        }
                    }
                };

                Some(super::v2::DeckEvent::Language(super::v2::LanguageEvent {
                    target_language: lang_event.target_language,
                    native_language: lang_event.native_language,
                    content,
                }))
            }
        }
    }
}
