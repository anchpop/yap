use rustc_hash::FxHashMap;
use std::collections::BTreeSet;

use chrono::Utc;
use language_utils::Lexeme;
use language_utils::PartOfSpeech;
use lasso::Spur;
use ordered_float::NotNan;

use crate::{
    CARD_TYPES, CardIndicator, CardStatus, CardType, ChallengeRequirements, Context, Deck,
    Regressions,
};

pub(crate) struct NextCardsIterator<'a> {
    pub(crate) cards: FxHashMap<CardIndicator<Spur>, CardStatus>,
    pub(crate) allowed_cards: AllowedCards,
    pub(crate) context: &'a Context,
    pub(crate) regressions: &'a Regressions,
    // Cached counts to avoid repeated iteration
    added_count: usize,
    card_type_counts: FxHashMap<CardType, u32>,
}

#[derive(Debug)]
pub(crate) enum AllowedCards {
    #[expect(unused)]
    // All is not yet used, but could be used to express intent more clearly than an empty BannedRequirements set
    All,
    BannedRequirements(std::collections::BTreeSet<ChallengeRequirements>),
    Type(CardType),
}

impl<'a> NextCardsIterator<'a> {
    pub fn new(deck: &'a Deck, allowed_cards: AllowedCards) -> Self {
        let cards = deck.cards.clone();

        // Initialize counts by iterating once
        let mut added_count = 0;
        let mut card_type_counts: FxHashMap<CardType, u32> =
            CARD_TYPES.iter().map(|card_type| (*card_type, 0)).collect();

        for (card, status) in &cards {
            if matches!(status, CardStatus::Tracked(_)) {
                added_count += 1;
                let card_type = card.card_type();
                card_type_counts
                    .entry(card_type)
                    .and_modify(|count| *count += 1);
            }
        }

        Self {
            cards,
            allowed_cards,
            context: &deck.context,
            regressions: &deck.regressions,
            added_count,
            card_type_counts,
        }
    }

    fn next_text_card(&self) -> Option<(CardIndicator<Spur>, rs_fsrs::Card)> {
        // None of the first 20 cards can be multiword cards
        // And the first 5 should be "easy"
        let no_auxiliary_words = self.added_count > 30;
        let no_multiwords = self.added_count > 20;
        let easy_cards = self.added_count < 5 && !self.context.course.teaches_new_writing_system();

        self.cards
            .iter()
            .filter_map(|(card, status)| {
                let CardIndicator::TargetLanguage { lexeme } = card else {
                    return None;
                };
                let mut preferred = true;
                if !no_multiwords && lexeme.multiword().is_some() {
                    preferred = false;
                }
                if easy_cards
                    && let Some(heteronym) = lexeme.heteronym()
                    && !self.context.is_word_easy(heteronym)
                {
                    preferred = false;
                }
                if no_auxiliary_words
                    && let Some(heteronym) = lexeme.heteronym()
                    && heteronym.pos == PartOfSpeech::Aux
                {
                    preferred = false;
                }

                status.unadded()?;

                let value =
                    self.context
                        .get_card_value_with_status(card, status, self.regressions)?;

                let fsrs_card = rs_fsrs::Card::new(Utc::now());

                Some((lexeme, fsrs_card, (preferred, value)))
            })
            .max_by_key(|(_, _, value)| *value)
            .map(|(card, fsrs_card, _)| {
                (CardIndicator::TargetLanguage { lexeme: *card }, fsrs_card)
            })
    }

    fn next_letter_pronunciation_card(&self) -> Option<(CardIndicator<Spur>, rs_fsrs::Card)> {
        // Find pronunciation patterns that haven't been added yet
        self.cards
            .iter()
            .filter_map(|(card, status)| {
                let CardIndicator::LetterPronunciation { pattern, position } = card else {
                    return None;
                };

                status.unadded()?;

                let value =
                    self.context
                        .get_card_value_with_status(card, status, self.regressions)?;

                let fsrs_card = rs_fsrs::Card::new(Utc::now());

                Some((pattern, position, fsrs_card, value))
            })
            .max_by_key(|(_, _, _, value)| *value)
            .map(|(pattern, position, fsrs_card, _)| {
                (
                    CardIndicator::LetterPronunciation {
                        pattern: *pattern,
                        position: *position,
                    },
                    fsrs_card,
                )
            })
    }

    fn next_listening_card(&self) -> Option<(CardIndicator<Spur>, rs_fsrs::Card)> {
        // Get all known words (already added text cards)
        let known_words: BTreeSet<Lexeme<Spur>> = self
            .cards
            .iter()
            .filter_map(|(card, status)| {
                if let CardIndicator::TargetLanguage { lexeme } = card {
                    if matches!(status, CardStatus::Tracked(_)) {
                        Some(*lexeme)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        self.cards
            .iter()
            .filter_map(|(card, status)| {
                let CardIndicator::ListeningHomophonous { pronunciation } = card else {
                    return None;
                };

                status.unadded()?;

                let value =
                    self.context
                        .get_card_value_with_status(card, status, self.regressions)?;

                // Check if we know at least one word with this pronunciation
                let has_known_word = self
                    .context
                    .language_pack
                    .pronunciation_to_lexemes(pronunciation)
                    .any(|(_, lexeme)| known_words.contains(&lexeme));

                // Only include if we know at least one word with this pronunciation
                if !has_known_word {
                    return None;
                }

                let fsrs_card = rs_fsrs::Card::new(Utc::now());

                Some((pronunciation, fsrs_card, value))
            })
            .max_by_key(|(_, _, value)| *value)
            .map(|(pronunciation, fsrs_card, _)| {
                (
                    CardIndicator::ListeningHomophonous {
                        pronunciation: *pronunciation,
                    },
                    fsrs_card,
                )
            })
    }
}

impl NextCardsIterator<'_> {
    fn next_card(&self) -> Option<(CardIndicator<Spur>, rs_fsrs::Card)> {
        if self.added_count < 20 {
            let can_only_add_text_cards = match &self.allowed_cards {
                AllowedCards::All | AllowedCards::Type(CardType::TargetLanguage) => true,
                AllowedCards::BannedRequirements(r) => r.is_empty(),
                _ => false,
            };
            if can_only_add_text_cards {
                let card = self.next_text_card()?;
                return Some(card);
            }
        }

        log::info!("next_card: self.allowed_cards={:?}", self.allowed_cards);

        // Calculate which type is most underrepresented based on target ratios
        let total_cards: u32 = self.card_type_counts.values().cloned().sum();
        let next_card_types = {
            let mut card_type_ratios = self
                .card_type_counts
                .iter()
                .filter(|(card_type, _)| match &self.allowed_cards {
                    AllowedCards::All => true,
                    AllowedCards::BannedRequirements(banned_requirements) => {
                        !banned_requirements.contains(&card_type.challenge_type())
                    }
                    AllowedCards::Type(allowed_card_type) => **card_type == *allowed_card_type,
                })
                .map(|(card_type, count)| {
                    (*card_type, {
                        let target_ratio = match card_type {
                            CardType::TargetLanguage => 0.65,
                            CardType::Listening => 0.3,
                            CardType::LetterPronunciation => 0.05,
                        };
                        (*count as f64 / total_cards as f64) / target_ratio
                    })
                })
                .collect::<Vec<(CardType, f64)>>();
            card_type_ratios.sort_by_key(|(_, ratio)| NotNan::new(*ratio).unwrap());
            card_type_ratios
                .into_iter()
                .map(|(card_type, _)| card_type)
                .collect::<Vec<_>>()
        };

        // Try to get a card of each type in priority order
        for card_type in next_card_types {
            let card = match card_type {
                CardType::TargetLanguage => self.next_text_card(),
                CardType::Listening => self.next_listening_card(),
                CardType::LetterPronunciation => self.next_letter_pronunciation_card(),
            };
            if let Some(card) = card {
                return Some(card);
            }
        }
        None
    }
}

impl Iterator for NextCardsIterator<'_> {
    type Item = CardIndicator<Spur>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((card, fsrs_card)) = self.next_card() {
            self.cards.insert(
                card,
                CardStatus::Tracked(crate::CardData::Added { fsrs_card }),
            );

            // Update incremental counts
            self.added_count += 1;
            let card_type = card.card_type();
            self.card_type_counts
                .entry(card_type)
                .and_modify(|count| *count += 1);

            Some(card)
        } else {
            None
        }
    }
}
