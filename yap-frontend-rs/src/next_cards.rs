use std::collections::BTreeSet;

use rustc_hash::FxHashMap;

use chrono::Utc;
use language_utils::{Atom, SpurGram, Word, WordType, grm};
use lasso::Spur;
use ordered_float::NotNan;

use crate::{CARD_TYPES, CardData, CardIndicator, CardType, ChallengeRequirements, Deck};

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

pub(crate) struct NextCardsIterator {
    /// Only tracked cards (Added/Ghost). Unadded cards are derived from context.
    pub(crate) cards: FxHashMap<CardIndicator<SpurGram, Spur>, CardData>,
    pub(crate) allowed_cards: AllowedCards,
    // Cached counts to avoid repeated iteration
    added_count: usize,
    card_type_counts: FxHashMap<CardType, u32>,
    // Precomputed sorted lists (value desc)
    text_values: Vec<(NotNan<f32>, SpurGram)>,
    /// Indices into text_values for single-word grams (only if added_count < 20 at construction)
    single_word_indices: Option<Vec<usize>>,
    /// Indices into text_values for easy single-word grams (only if added_count < 5 and !teaches_new_writing_system)
    easy_single_word_indices: Option<Vec<usize>>,
    listening_values: Vec<(NotNan<f32>, SpurGram)>,
    pronunciation_values: Vec<(NotNan<f32>, CardIndicator<SpurGram, Spur>)>,
}

#[derive(Debug)]
pub(crate) enum AllowedCards {
    #[expect(unused)]
    // All is not yet used, but could be used to express intent more clearly than an empty BannedRequirements set
    All,
    BannedRequirements(BTreeSet<ChallengeRequirements>),
    Type(CardType),
}

impl NextCardsIterator {
    pub fn new(deck: &Deck, allowed_cards: AllowedCards) -> Self {
        let cards = deck.cards.clone();
        let context = &deck.context;
        let regressions = &deck.regressions;

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

        // Precompute text card values: all unadded grams sorted by value desc
        let mut text_values: Vec<(NotNan<f32>, SpurGram)> = context
            .language_pack
            .gram_frequencies
            .keys()
            .filter_map(|gram| {
                let card = CardIndicator::WrittenGram { gram: *gram };
                if cards.contains_key(&card) {
                    return None;
                }
                let value = context.get_card_value_with_status(&card, None, regressions)?;
                Some((value, *gram))
            })
            .collect();
        text_values.sort_by(|a, b| b.0.cmp(&a.0));

        // Build single-word indices if needed for early onboarding preference
        let single_word_indices = if added_count < 20 {
            Some(
                text_values
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, gram))| {
                        gram_single_word(context.language_pack.gram_rodeo.resolve(gram)).is_some()
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<usize>>(),
            )
        } else {
            None
        };

        // Build easy single-word indices (subset of single_word_indices)
        let easy_single_word_indices =
            if added_count < 5 && !context.course.teaches_new_writing_system() {
                single_word_indices.as_ref().map(|swi| {
                    swi.iter()
                        .copied()
                        .filter(|&i| {
                            let gram = &text_values[i].1;
                            let resolved = context.language_pack.gram_rodeo.resolve(gram);
                            match gram_single_word(resolved) {
                                Some(word) => match &word.word_type {
                                    WordType::Heteronym(h) => context.is_word_easy(h),
                                    _ => true,
                                },
                                None => false,
                            }
                        })
                        .collect::<Vec<usize>>()
                })
            } else {
                None
            };

        // Precompute listening card values: all unadded listening grams sorted by value desc
        let mut listening_values: Vec<(NotNan<f32>, SpurGram)> = context
            .language_pack
            .gram_frequencies
            .keys()
            .filter_map(|gram| {
                let card = CardIndicator::ListeningGram { gram: *gram };
                if cards.contains_key(&card) {
                    return None;
                }
                let value = context.get_card_value_with_status(&card, None, regressions)?;
                Some((value, *gram))
            })
            .collect();
        listening_values.sort_by(|a, b| b.0.cmp(&a.0));

        // Precompute pronunciation card values
        let mut pronunciation_values: Vec<(NotNan<f32>, CardIndicator<SpurGram, Spur>)> = context
            .language_pack
            .pronunciation_data
            .guides
            .iter()
            .filter_map(|guide| {
                let pattern = context.language_pack.string_rodeo.get(&guide.pattern)?;
                let card = CardIndicator::LetterPronunciation {
                    pattern,
                    position: guide.position,
                };
                if cards.contains_key(&card) {
                    return None;
                }
                let value = context.get_card_value_with_status(&card, None, regressions)?;
                Some((value, card))
            })
            .collect();
        pronunciation_values.sort_by(|a, b| b.0.cmp(&a.0));

        Self {
            cards,
            allowed_cards,
            added_count,
            card_type_counts,
            text_values,
            single_word_indices,
            easy_single_word_indices,
            listening_values,
            pronunciation_values,
        }
    }

    fn next_text_card(&self) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        // Try preferred indices first based on added_count thresholds
        let preferred_gram = if self.added_count < 5 {
            // easy_single_word_indices is Some only when !teaches_new_writing_system at construction
            if let Some(easy_indices) = &self.easy_single_word_indices {
                self.first_unadded_text_gram(easy_indices)
            } else if let Some(sw_indices) = &self.single_word_indices {
                // teaches_new_writing_system case: prefer any single-word
                self.first_unadded_text_gram(sw_indices)
            } else {
                None
            }
        } else if self.added_count < 20 {
            if let Some(sw_indices) = &self.single_word_indices {
                self.first_unadded_text_gram(sw_indices)
            } else {
                None
            }
        } else {
            None
        };

        // Fallback to best overall gram
        let gram = preferred_gram.or_else(|| {
            self.text_values.iter().find_map(|(_, gram)| {
                let card = CardIndicator::WrittenGram { gram: *gram };
                if self.cards.contains_key(&card) {
                    None
                } else {
                    Some(*gram)
                }
            })
        });

        gram.map(|gram| {
            (
                CardIndicator::WrittenGram { gram },
                rs_fsrs::Card::new(Utc::now()),
            )
        })
    }

    fn first_unadded_text_gram(&self, indices: &[usize]) -> Option<SpurGram> {
        indices.iter().find_map(|&i| {
            let gram = self.text_values[i].1;
            let card = CardIndicator::WrittenGram { gram };
            if self.cards.contains_key(&card) {
                None
            } else {
                Some(gram)
            }
        })
    }

    fn next_letter_pronunciation_card(
        &self,
    ) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        self.pronunciation_values.iter().find_map(|(_, card)| {
            if self.cards.contains_key(card) {
                None
            } else {
                Some((*card, rs_fsrs::Card::new(Utc::now())))
            }
        })
    }

    fn next_listening_card(&self) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        self.listening_values.iter().find_map(|(_, gram)| {
            let card = CardIndicator::ListeningGram { gram: *gram };
            if self.cards.contains_key(&card) {
                return None;
            }
            // Only include if we already know this gram as a written card
            if !self
                .cards
                .contains_key(&CardIndicator::WrittenGram { gram: *gram })
            {
                return None;
            }
            Some((card, rs_fsrs::Card::new(Utc::now())))
        })
    }
}

impl NextCardsIterator {
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

impl Iterator for NextCardsIterator {
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
