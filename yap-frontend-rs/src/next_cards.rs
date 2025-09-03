use std::collections::BTreeSet;

use language_utils::Lexeme;
use lasso::Spur;

use crate::{CardIndicator, ChallengeType, Deck, LanguagePack};

pub struct NextCardsIterator<'a> {
    pub cards: Vec<CardIndicator<Spur>>,
    pub permitted_types: Vec<ChallengeType>,
    language_pack: &'a LanguagePack,
}

impl<'a> NextCardsIterator<'a> {
    pub fn new(state: &'a Deck, permitted_types: Vec<ChallengeType>) -> Self {
        Self {
            cards: state.cards.keys().cloned().collect(),
            permitted_types,
            language_pack: &state.language_pack,
        }
    }

    fn next_text_card(&self) -> Option<CardIndicator<Spur>> {
        let known_words: BTreeSet<Lexeme<Spur>> = self
            .cards
            .iter()
            .filter_map(CardIndicator::target_language)
            .cloned()
            .collect();
        for lexeme in self.language_pack.word_frequencies.keys() {
            if known_words.contains(lexeme) {
                continue;
            }
            if self.cards.len() < 20 && lexeme.multiword().is_some() {
                continue;
            }
            return Some(CardIndicator::TargetLanguage { lexeme: *lexeme });
        }
        None
    }

    fn next_listening_card(&self) -> Option<CardIndicator<Spur>> {
        let known_pronunciations: BTreeSet<Spur> = self
            .cards
            .iter()
            .filter_map(CardIndicator::listening_homophonous)
            .copied()
            .collect();
        let known_words: BTreeSet<Lexeme<Spur>> = self
            .cards
            .iter()
            .filter_map(CardIndicator::target_language)
            .cloned()
            .collect();
        for lexeme in self.language_pack.word_frequencies.keys() {
            if !known_words.contains(lexeme) {
                continue;
            }
            let heteronym = match lexeme.heteronym() {
                Some(h) => h,
                None => continue,
            };
            let Some(&pronunciation) = self
                .language_pack
                .word_to_pronunciation
                .get(&heteronym.word)
            else {
                log::error!(
                    "Word {heteronym:?} was in the deck, but was not found in word_to_pronunciation",
                    heteronym = heteronym.resolve(&self.language_pack.rodeo)
                );
                continue;
            };
            if known_pronunciations.contains(&pronunciation) {
                continue;
            }
            return Some(CardIndicator::ListeningHomophonous { pronunciation });
        }
        None
    }
}

impl Iterator for NextCardsIterator<'_> {
    type Item = CardIndicator<Spur>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.permitted_types.is_empty() {
            return None;
        }

        if self.permitted_types.len() == 1 {
            let card = match self.permitted_types[0] {
                ChallengeType::Text => self.next_text_card(),
                ChallengeType::Listening => self.next_listening_card(),
            }?;
            self.cards.push(card.clone());
            return Some(card);
        }

        if self.cards.len() < 20 {
            let card = self.next_text_card()?;
            self.cards.push(card.clone());
            return Some(card);
        }

        let text_count = self
            .cards
            .iter()
            .filter(|c| matches!(c, CardIndicator::TargetLanguage { .. }))
            .count();
        let listening_count = self
            .cards
            .iter()
            .filter(|c| matches!(c, CardIndicator::ListeningHomophonous { .. }))
            .count();

        let desired = if listening_count < text_count / 2 {
            ChallengeType::Listening
        } else {
            ChallengeType::Text
        };

        let other = if desired == ChallengeType::Text {
            ChallengeType::Listening
        } else {
            ChallengeType::Text
        };

        for ty in [desired, other] {
            let card = match ty {
                ChallengeType::Text => self.next_text_card(),
                ChallengeType::Listening => self.next_listening_card(),
            };
            if let Some(card) = card {
                self.cards.push(card.clone());
                return Some(card);
            }
        }
        None
    }
}
