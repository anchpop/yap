use std::collections::BTreeSet;

use rustc_hash::FxHashMap;

use chrono::Utc;
use language_utils::{Atom, SpurGram, Word, WordType, grm};
use lasso::Spur;
use ordered_float::NotNan;

use crate::{
    CARD_TYPES, CardData, CardIndicator, CardType, ChallengeRequirements, Context, Deck,
    Regressions,
};

/// Returns the single word from a gram if it has exactly one Tok atom.
/// Returns None for multi-word grams or empty grams.
fn gram_single_word(gram: &grm<Spur>) -> Option<&Word<Spur>> {
    let mut words = gram.iter().filter_map(|atom| match atom {
        Atom::Tok(word) => Some(word),
        Atom::Control(_) => None,
    });
    let first = words.next()?;
    if words.next().is_some() {
        None // More than one word
    } else {
        Some(first)
    }
}

pub(crate) struct NextCardsIterator<'a> {
    /// Only tracked cards (Added/Ghost). Unadded cards are derived from context.
    pub(crate) cards: FxHashMap<CardIndicator<SpurGram, Spur>, CardData>,
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
    BannedRequirements(BTreeSet<ChallengeRequirements>),
    Type(CardType),
}

impl<'a> NextCardsIterator<'a> {
    pub fn new(deck: &'a Deck, allowed_cards: AllowedCards) -> Self {
        let cards = deck.cards.clone();

        // Initialize counts by iterating once over tracked cards
        let mut added_count = 0;
        let mut card_type_counts: FxHashMap<CardType, u32> =
            CARD_TYPES.iter().map(|card_type| (*card_type, 0)).collect();

        for card in cards.keys() {
            added_count += 1;
            let card_type = card.card_type();
            card_type_counts
                .entry(card_type)
                .and_modify(|count| *count += 1);
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

    fn next_text_card(&self) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        // First 20 cards: prefer single-word grams (deprioritize multi-word grams/phrases)
        // First 5 cards: prefer "easy" single-word grams
        let prefer_single_word = self.added_count < 20;
        let easy_only = self.added_count < 5 && !self.context.course.teaches_new_writing_system();

        // Iterate all grams from context, skip ones already tracked
        self.context
            .language_pack
            .gram_frequencies
            .keys()
            .filter_map(|gram| {
                let card = CardIndicator::WrittenGram { gram: *gram };
                // Skip if already tracked
                if self.cards.contains_key(&card) {
                    return None;
                }

                let mut preferred = true;

                if let Some(word) =
                    gram_single_word(self.context.language_pack.gram_rodeo.resolve(gram))
                {
                    // Single-word gram: check if it's easy
                    if easy_only
                        && let WordType::Heteronym(h) = &word.word_type
                        && !self.context.is_word_easy(h)
                    {
                        preferred = false;
                    }
                } else {
                    // Multi-word gram: deprioritize during early onboarding
                    if prefer_single_word {
                        preferred = false;
                    }
                }

                let value =
                    self.context
                        .get_card_value_with_status(&card, None, self.regressions)?;

                Some((card, (preferred, value)))
            })
            .max_by_key(|(_, value)| *value)
            .map(|(card, _)| (card, rs_fsrs::Card::new(Utc::now())))
    }

    fn next_letter_pronunciation_card(
        &self,
    ) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        // Find pronunciation patterns that haven't been added yet
        self.context
            .language_pack
            .pronunciation_data
            .guides
            .iter()
            .filter_map(|guide| {
                let pattern = self
                    .context
                    .language_pack
                    .string_rodeo
                    .get(&guide.pattern)?;
                let card = CardIndicator::LetterPronunciation {
                    pattern,
                    position: guide.position,
                };

                // Skip if already tracked
                if self.cards.contains_key(&card) {
                    return None;
                }

                let value =
                    self.context
                        .get_card_value_with_status(&card, None, self.regressions)?;

                Some((card, value))
            })
            .max_by_key(|(_, value)| *value)
            .map(|(card, _)| (card, rs_fsrs::Card::new(Utc::now())))
    }

    fn next_listening_card(&self) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        // Get all known grams (already added WrittenGram cards)
        let known_grams: BTreeSet<SpurGram> = self
            .cards
            .iter()
            .filter_map(|(card, _)| {
                if let CardIndicator::WrittenGram { gram } = card {
                    Some(*gram)
                } else {
                    None
                }
            })
            .collect();

        // Iterate all grams from context, skip ones already tracked
        self.context
            .language_pack
            .gram_frequencies
            .keys()
            .filter_map(|gram| {
                let card = CardIndicator::ListeningGram { gram: *gram };

                // Skip if already tracked
                if self.cards.contains_key(&card) {
                    return None;
                }

                // Only include if we already know this gram as a written card
                if !known_grams.contains(gram) {
                    return None;
                }

                let value =
                    self.context
                        .get_card_value_with_status(&card, None, self.regressions)?;

                Some((*gram, value))
            })
            .max_by_key(|(_, value)| *value)
            .map(|(gram, _)| {
                (
                    CardIndicator::ListeningGram { gram },
                    rs_fsrs::Card::new(Utc::now()),
                )
            })
    }
}

impl NextCardsIterator<'_> {
    fn next_card(&self) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
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
    type Item = CardIndicator<SpurGram, Spur>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((card, fsrs_card)) = self.next_card() {
            // Get card_type before moving the card
            let card_type = card.card_type();

            self.cards
                .insert(card, crate::CardData::Added { fsrs_card });

            // Update incremental counts
            self.added_count += 1;
            self.card_type_counts
                .entry(card_type)
                .and_modify(|count| *count += 1);

            Some(card)
        } else {
            None
        }
    }
}
