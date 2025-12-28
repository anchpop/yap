use std::collections::BTreeSet;

use language_utils::{Lexeme, TtsProvider, TtsRequest, transcription_challenge};
use lasso::Spur;

use crate::{
    AudioRequest, CardContent, CardData, CardIndicator, CardStatus, Challenge, Deck, ReviewInfo,
    TranscribeComprehensibleSentence,
};

impl Deck {
    pub(crate) fn get_homophonous_listening_challenge(
        &self,
        review_info: &ReviewInfo,
        card_indicator: CardIndicator<Spur>,
        is_new: bool,
        pronunciation: Spur,
    ) -> Challenge<Spur> {
        let flashcard = {
            let listening_prefix =
                ReviewInfo::get_listening_prefix(self.context.target_language).to_string();
            let possible_words: Vec<(bool, Spur)> = {
                let Some(possible_words) = self
                    .context
                    .language_pack
                    .pronunciation_to_words
                    .get(&pronunciation)
                    .cloned()
                else {
                    panic!(
                        "Pronunciation {:?} was in the deck, but was not found in pronunciation_to_words",
                        self.context.language_pack.rodeo.resolve(&pronunciation)
                    );
                };
                let possible_words = possible_words.into_iter().collect::<BTreeSet<_>>();

                // figure out which of those words the user knows
                possible_words
                    .iter()
                    .map(|word| {
                        // Check if any lexeme for this word is known
                        let word_known = self
                            .context
                            .language_pack
                            .pronunciation_to_lexemes(&pronunciation)
                            .filter(|(w, _)| w == word)
                            .any(|(_, lexeme)| {
                                self.cards
                                    .get(&CardIndicator::TargetLanguage { lexeme })
                                    .is_some_and(|status| match status {
                                        CardStatus::Tracked(CardData::Added { fsrs_card })
                                        | CardStatus::Tracked(CardData::Ghost { fsrs_card }) => {
                                            fsrs_card.state != rs_fsrs::State::New
                                        }
                                        _ => false,
                                    })
                            });
                        (word_known, *word)
                    })
                    .collect()
            };
            let audio = AudioRequest {
                request: TtsRequest {
                    text: format!(
                        "{}... \"{}\".",
                        listening_prefix,
                        self.context.language_pack.rodeo.resolve(
                            &possible_words
                                .iter()
                                .find(|(known, _)| *known)
                                .or(possible_words.first())
                                .cloned()
                                .unwrap()
                                .1
                        )
                    ),
                    language: self.context.target_language,
                },
                provider: TtsProvider::Google,
            };
            Challenge::<Spur>::FlashCardReview {
                indicator: card_indicator,
                audio: Some(audio),
                content: CardContent::Listening {
                    pronunciation,
                    possible_words,
                },
                is_new,
                listening_prefix: Some(listening_prefix),
            }
        };
        if is_new {
            flashcard
        } else {
            let mut heteronyms = self
                .context
                .language_pack
                .pronunciation_to_words
                .get(&pronunciation)
                .unwrap()
                .iter()
                .cloned()
                .flat_map(|word| {
                    self.context
                        .language_pack
                        .words_to_heteronyms
                        .get(&word)
                        .unwrap()
                        .clone()
                })
                .filter(|heteronym| self.lexeme_known(&Lexeme::Heteronym(*heteronym)))
                .collect::<Vec<_>>();
            heteronyms
                .sort_by_key(|heteronym| self.stats.words_listened_to.get(heteronym).unwrap_or(&0));

            if let Some((target_heteronym, sentence)) = heteronyms
                .iter()
                .filter_map(|heteronym| {
                    let comprehensible_lexemes =
                        review_info.get_comprehensible_written_lexemes(self);
                    let sentence = self.get_comprehensible_sentence_containing(
                        Some(&Lexeme::Heteronym(*heteronym)),
                        comprehensible_lexemes,
                        &self.stats.sentences_reviewed,
                        &self.context.language_pack,
                    )?;
                    Some((*heteronym, sentence))
                })
                .next()
            {
                let parts = sentence
                    .target_language_literals
                    .into_iter()
                    .map(|literal| {
                        if let Some(heteronym) = literal.heteronym()
                            && *heteronym == target_heteronym
                        {
                            transcription_challenge::Part::AskedToTranscribe {
                                parts: vec![literal.resolve(&self.context.language_pack.rodeo)],
                            }
                        } else {
                            transcription_challenge::Part::Provided {
                                part: literal.resolve(&self.context.language_pack.rodeo),
                            }
                        }
                    })
                    .collect();

                // Get movie titles from sentence_sources and movie metadata
                let movie_titles = self
                    .context
                    .language_pack
                    .sentence_sources
                    .get(&sentence.target_language)
                    .map(|source| {
                        source
                            .movie_ids
                            .iter()
                            .filter_map(|movie_id| {
                                self.context
                                    .language_pack
                                    .movies
                                    .get(movie_id)
                                    .map(|metadata| (movie_id.clone(), metadata.title.clone()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                Challenge::TranscribeComprehensibleSentence(TranscribeComprehensibleSentence {
                    target_language: sentence.target_language,
                    native_language: *sentence.native_languages.first().unwrap(),
                    parts,
                    audio: AudioRequest {
                        request: TtsRequest {
                            text: self
                                .context
                                .language_pack
                                .rodeo
                                .resolve(&sentence.target_language)
                                .to_string(),
                            language: self.context.target_language,
                        },
                        provider: TtsProvider::Google,
                    },
                    movie_titles,
                })
            } else {
                flashcard
            }
        }
    }
}
