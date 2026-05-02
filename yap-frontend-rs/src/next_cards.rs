use std::collections::BTreeSet;

use rustc_hash::FxHashMap;

use chrono::Utc;
use language_utils::{Atom, SpurGram, Word, grm};
use lasso::Spur;
use ordered_float::NotNan;

use crate::{
    CARD_TYPES, CardData, CardIndicator, CardType, ChallengeRequirements, Deck,
    deck_event::current::GoalSelection,
};

/// Partition the top `n` elements to the front (descending by first tuple element),
/// then sort only those. Much faster than a full sort when `n << len`.
fn partial_sort_desc<T: Ord>(values: &mut [(NotNan<f32>, T)], n: usize) {
    let compare =
        |a: &(NotNan<f32>, T), b: &(NotNan<f32>, T)| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1));

    if n == 0 || values.len() <= n {
        values.sort_by(compare);
    } else {
        values.select_nth_unstable_by(n - 1, compare);
        values[..n].sort_by(compare);
    }
}

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
    /// Single-word grams sorted by value descending (only if added_count < 20).
    /// Single-word is a hard onboarding constraint, but the order *within* the
    /// constraint set respects the regression so the placement test influences
    /// onboarding instead of being silently overridden by frequency order.
    single_word_grams: Option<Vec<(NotNan<f32>, SpurGram)>>,
    /// Easy single-word grams sorted by value descending (only if added_count < 5
    /// and !teaches_new_writing_system). Same reasoning as `single_word_grams`.
    easy_single_word_grams: Option<Vec<(NotNan<f32>, SpurGram)>>,
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
    pub fn new(
        deck: &Deck,
        allowed_cards: AllowedCards,
        goal: &Option<GoalSelection>,
        limit: usize,
    ) -> Self {
        // Buffer beyond the requested limit to account for cards split across types
        // (text vs listening vs pronunciation) and the iterator's balancing logic.
        let early_term_n = (limit * 3).max(100);

        let comprehensible_written = deck.get_comprehensible_written_grams(true).clone();
        let cards = deck.cards.clone();
        let context = &deck.context;
        let regressions = &deck.regressions;

        // Determine which frequency list to use based on goal
        let goal_freq_list = goal.as_ref().and_then(|g| {
            let source_id = match g {
                GoalSelection::Movie { id } => language_utils::FrequencySourceId::Movie(id.clone()),
                GoalSelection::PimsleurLesson { level, lesson } => {
                    language_utils::FrequencySourceId::PimsleurLesson(
                        language_utils::PimsleurLesson {
                            level: *level,
                            lesson: *lesson,
                        },
                    )
                }
            };
            context
                .language_pack
                .source_gram_frequencies
                .get(&source_id)
        });

        let gram_source = goal_freq_list.unwrap_or(&context.language_pack.gram_frequencies);

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

        // Single fused pass over gram_source.entries that fills three top-N
        // value rankings of unadded WrittenGrams:
        //   - text_values:                no constraint (post-onboarding pool)
        //   - single_word_values:         single-word grams        (cards 5..20)
        //   - easy_single_word_values:    easy single-word grams   (cards 0..5)
        //
        // Per-iteration we compute the regression-based value once, then update
        // each enabled heap. The single-word predicate is computed once per gram
        // when either of its dependent heaps is still active.
        //
        // gram_source.entries is sorted by count descending, so freq_score is also
        // descending. Since value = (1 - probability) * freq_score ≤ freq_score,
        // once freq_score drops below a heap's smallest top-N value, no future gram
        // can enter that heap. The loop terminates only when *every* enabled heap
        // is in that state.
        let need_single_word = added_count < 20;
        let need_easy_single_word =
            added_count < 5 && !context.course.teaches_new_writing_system();

        let mut text_values: Vec<(NotNan<f32>, SpurGram)> = Vec::new();
        let mut text_top_n: std::collections::BinaryHeap<std::cmp::Reverse<NotNan<f32>>> =
            std::collections::BinaryHeap::new();
        let mut single_word_values: Vec<(NotNan<f32>, SpurGram)> = Vec::new();
        let mut single_top_n: std::collections::BinaryHeap<std::cmp::Reverse<NotNan<f32>>> =
            std::collections::BinaryHeap::new();
        let mut easy_single_word_values: Vec<(NotNan<f32>, SpurGram)> = Vec::new();
        let mut easy_top_n: std::collections::BinaryHeap<std::cmp::Reverse<NotNan<f32>>> =
            std::collections::BinaryHeap::new();

        // Inline helper to push a candidate into a (Vec, top-N heap) pair, maintaining
        // the heap as a min-heap of size at most early_term_n.
        let push_top_n =
            |values: &mut Vec<(NotNan<f32>, SpurGram)>,
             top_n: &mut std::collections::BinaryHeap<std::cmp::Reverse<NotNan<f32>>>,
             gram: SpurGram,
             value: NotNan<f32>| {
                values.push((value, gram));
                if top_n.len() < early_term_n {
                    top_n.push(std::cmp::Reverse(value));
                } else if top_n.peek().is_some_and(|t| value > t.0) {
                    top_n.pop();
                    top_n.push(std::cmp::Reverse(value));
                }
            };

        for (gram, frequency) in gram_source.entries.iter() {
            let freq_score = NotNan::new(frequency.frequency_score()).unwrap_or_default();

            // A heap is "saturated" when no future gram can enter it.
            // Disabled heaps are vacuously saturated.
            let text_saturated = text_top_n.len() >= early_term_n
                && text_top_n.peek().is_some_and(|t| freq_score < t.0);
            let single_saturated = !need_single_word
                || (single_top_n.len() >= early_term_n
                    && single_top_n.peek().is_some_and(|t| freq_score < t.0));
            let easy_saturated = !need_easy_single_word
                || (easy_top_n.len() >= early_term_n
                    && easy_top_n.peek().is_some_and(|t| freq_score < t.0));

            if text_saturated && single_saturated && easy_saturated {
                break;
            }

            let card = CardIndicator::WrittenGram { gram: *gram };
            if matches!(cards.get(&card), Some(CardData::Added { .. })) {
                continue;
            }

            let Some(value) = context
                .card_value_with_frequency(&card, cards.get(&card), regressions, *frequency)
            else {
                continue;
            };

            if !text_saturated {
                push_top_n(&mut text_values, &mut text_top_n, *gram, value);
            }

            // Compute the single-word predicate at most once per gram, and only
            // when at least one constraint heap still cares.
            let single_active = need_single_word && !single_saturated;
            let easy_active = need_easy_single_word && !easy_saturated;
            if single_active || easy_active {
                let is_single_word =
                    gram_single_word(context.language_pack.gram_rodeo.resolve(gram)).is_some();
                if is_single_word {
                    if single_active {
                        push_top_n(&mut single_word_values, &mut single_top_n, *gram, value);
                    }
                    if easy_active && frequency.easy {
                        push_top_n(
                            &mut easy_single_word_values,
                            &mut easy_top_n,
                            *gram,
                            value,
                        );
                    }
                }
            }
        }

        partial_sort_desc(&mut text_values, early_term_n);
        let single_word_grams = if need_single_word {
            partial_sort_desc(&mut single_word_values, early_term_n);
            Some(single_word_values)
        } else {
            None
        };
        let easy_single_word_grams = if need_easy_single_word {
            partial_sort_desc(&mut easy_single_word_values, early_term_n);
            Some(easy_single_word_values)
        } else {
            None
        };

        // Precompute listening card values: grams not yet Added, partially sorted by value desc.
        // Ghost cards are included since they can be promoted to Added.
        // Pre-filter by comprehensible_written so the partial sort only contains
        // usable values (otherwise the comprehensible filter in next_listening_card
        // can skip past the sorted portion).
        let mut listening_values: Vec<(NotNan<f32>, SpurGram)> = gram_source
            .entries
            .iter()
            .filter_map(|(gram, frequency)| {
                if !comprehensible_written.contains(gram) {
                    return None;
                }
                let card = CardIndicator::ListeningGram { gram: *gram };
                if matches!(cards.get(&card), Some(CardData::Added { .. })) {
                    return None;
                }
                let value = context.card_value_with_frequency(
                    &card,
                    cards.get(&card),
                    regressions,
                    *frequency,
                )?;
                Some((value, *gram))
            })
            .collect();
        partial_sort_desc(&mut listening_values, early_term_n);

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
        pronunciation_values.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        Self {
            cards,
            allowed_cards,
            added_count,
            card_type_counts,
            text_values,
            single_word_grams,
            easy_single_word_grams,
            listening_values,
            pronunciation_values,
        }
    }

    /// Returns true if a card is Already Added (not Ghost, not absent).
    fn is_added(&self, card: &CardIndicator<SpurGram, Spur>) -> bool {
        matches!(self.cards.get(card), Some(CardData::Added { .. }))
    }

    fn next_text_card(&self) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        // Try preferred grams first based on added_count thresholds
        let preferred_gram = if self.added_count < 5 {
            if let Some(easy_grams) = &self.easy_single_word_grams {
                self.first_unadded_gram(easy_grams)
            } else if let Some(single_word_grams) = &self.single_word_grams {
                // teaches_new_writing_system case: prefer any single-word
                self.first_unadded_gram(single_word_grams)
            } else {
                None
            }
        } else if self.added_count < 20 {
            if let Some(single_word_grams) = &self.single_word_grams {
                self.first_unadded_gram(single_word_grams)
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
                if self.is_added(&card) {
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

    fn first_unadded_gram(&self, grams: &[(NotNan<f32>, SpurGram)]) -> Option<SpurGram> {
        grams.iter().find_map(|&(_value, gram)| {
            let card = CardIndicator::WrittenGram { gram };
            if self.is_added(&card) {
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
            if self.is_added(card) {
                None
            } else {
                Some((*card, rs_fsrs::Card::new(Utc::now())))
            }
        })
    }

    fn next_listening_card(&self) -> Option<(CardIndicator<SpurGram, Spur>, rs_fsrs::Card)> {
        // listening_values is pre-filtered to only comprehensible written grams
        self.listening_values.iter().find_map(|(_, gram)| {
            let card = CardIndicator::ListeningGram { gram: *gram };
            if self.is_added(&card) {
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
                        if total_cards == 0 {
                            0.0
                        } else {
                            let target_ratio = match card_type {
                                CardType::TargetLanguage => 0.65,
                                CardType::Listening => 0.33,
                                CardType::LetterPronunciation => 0.02,
                            };
                            (*count as f64 / total_cards as f64) / target_ratio
                        }
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{AllowedCards, NextCardsIterator, gram_single_word};
    use crate::{CardIndicator, Deck};

    #[test]
    fn test_first_20_cards_stay_single_word_when_available() {
        let deck = Deck::default();
        if deck
            .context
            .language_pack
            .gram_frequencies
            .entries
            .is_empty()
        {
            return;
        }

        let iter = NextCardsIterator::new(
            &deck,
            AllowedCards::BannedRequirements(BTreeSet::new()),
            &None,
            20,
        );
        let Some(single_word_grams) = &iter.single_word_grams else {
            return;
        };
        if single_word_grams.len() < 20 {
            return;
        }

        let cards: Vec<_> = iter.take(20).collect();
        assert_eq!(cards.len(), 20);
        for card in cards {
            let CardIndicator::WrittenGram { gram } = card else {
                panic!("expected written card during early onboarding");
            };
            assert!(
                gram_single_word(deck.context.language_pack.gram_rodeo.resolve(&gram)).is_some()
            );
        }
    }
}
