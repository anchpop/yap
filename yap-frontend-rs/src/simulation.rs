use crate::{Challenge, Deck, TranscribeComprehensibleSentence, TranslateComprehensibleSentence};
use chrono::{Duration, Utc};
use language_utils::transcription_challenge;
use weapon::AppState;
use weapon::data_model::Timestamped;

impl Deck {
    /// Simulate `days` of reviews, calling `on_challenge` for each generated challenge.
    /// The simulation answers every challenge perfectly, adds 10 new cards at the end of each day,
    /// and advances the time by one day.
    pub(crate) fn simulate_usage<F>(&self, days: u32, mut on_challenge: F)
    where
        F: FnMut(Challenge<String>),
    {
        let mut deck = self.clone();
        let mut now = Utc::now();
        let mut index = 0usize;

        for _day in 0..days {
            loop {
                let review_info = deck.get_review_info(vec![]);
                if let Some(card_index) = review_info.get_next_review_card() {
                    if let Some(challenge) = review_info.get_challenge_for_card(&deck, card_index) {
                        on_challenge(challenge.clone());
                        let event = match challenge {
                            Challenge::FlashCardReview { indicator, .. } => {
                                deck.review_card(indicator, "good".to_string())
                            }
                            Challenge::TranslateComprehensibleSentence(
                                TranslateComprehensibleSentence {
                                    target_language, ..
                                },
                            ) => deck.translate_sentence_perfect(target_language),
                            Challenge::TranscribeComprehensibleSentence(
                                TranscribeComprehensibleSentence { parts, .. },
                            ) => {
                                let graded = parts
                                    .into_iter()
                                    .map(|part| match part {
                                        transcription_challenge::Part::AskedToTranscribe { parts } => {
                                            let submission = parts
                                                .iter()
                                                .map(|p| p.text.clone())
                                                .collect::<Vec<_>>()
                                                .join(" ");
                                            transcription_challenge::PartGraded::AskedToTranscribe {
                                                submission,
                                                parts: parts
                                                    .into_iter()
                                                    .map(|p| transcription_challenge::PartGradedPart {
                                                        heard: p,
                                                        grade: transcription_challenge::WordGrade::Perfect {},
                                                    })
                                                    .collect(),
                                            }
                                        }
                                        transcription_challenge::Part::Provided { part } => {
                                            transcription_challenge::PartGraded::Provided { part }
                                        }
                                    })
                                    .collect();
                                deck.transcribe_sentence(graded)
                            }
                        };

                        if let Some(event) = event {
                            let ts = Timestamped {
                                timestamp: now,
                                within_device_events_index: index,
                                event,
                            };
                            deck = deck.apply_event(&ts);
                            index += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            if let Some(event) = deck.add_next_unknown_cards(None, 10) {
                let ts = Timestamped {
                    timestamp: now,
                    within_device_events_index: index,
                    event,
                };
                deck = deck.apply_event(&ts);
                index += 1;
            }

            now += Duration::days(1);
        }
    }
}
