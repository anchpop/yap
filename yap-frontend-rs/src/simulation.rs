use crate::Rating;
use crate::{
    Challenge, Deck, DeckState, TranscribeComprehensibleSentence, TranslateComprehensibleSentence,
};
use chrono::{DateTime, Duration, Utc};
use language_utils::{Gram, transcription_challenge};
use weapon::AppState;
use weapon::data_model::Timestamped;

fn apply_event(deck: Deck, event: &Timestamped<crate::DeckEvent>) -> Deck {
    let context = deck.context.clone();
    let state = DeckState::from(deck);
    let state = Deck::process_event(state, &context, event);
    Deck::finalize(state, &context)
}

/// Iterator that simulates daily usage of a deck, yielding all challenges for each day
pub struct DailySimulationIterator {
    deck: Deck,
    current_time: DateTime<Utc>,
    event_index: usize,
}

impl DailySimulationIterator {
    pub fn new(deck: Deck, current_time: DateTime<Utc>) -> Self {
        Self {
            deck,
            current_time,
            event_index: 0,
        }
    }
}

impl DailySimulationIterator {
    pub fn next(mut self) -> (Self, Vec<Challenge<Gram<String>>>) {
        let mut day_challenges = Vec::new();

        // Process all due reviews for the day (max 20 cards to keep simulation fast)
        const MAX_DAILY_CARDS: usize = 20;
        loop {
            if day_challenges.len() >= MAX_DAILY_CARDS {
                break;
            }

            let review_info = self
                .deck
                .get_review_info(vec![], self.current_time.timestamp_millis() as f64);
            if let Some(challenge) = review_info.get_next_challenge(&self.deck) {
                day_challenges.push(challenge.clone());

                // Answer the challenge, marking new flashcards as forgotten once
                let event = match challenge {
                    Challenge::FlashCardReview {
                        indicator, is_new, ..
                    } => {
                        let rating = if is_new {
                            Rating::Again
                        } else {
                            Rating::Remembered
                        };
                        self.deck.review_card(indicator, rating)
                    }
                    Challenge::TranslateComprehensibleSentence(
                        TranslateComprehensibleSentence {
                            target_language, ..
                        },
                    ) => self
                        .deck
                        .translate_sentence_perfect(vec![], target_language),
                    Challenge::TranscribeComprehensibleSentence(
                        TranscribeComprehensibleSentence { parts, .. },
                    ) => {
                        let graded = parts
                            .into_iter()
                            .map(|part| match part {
                                transcription_challenge::Part::AskedToTranscribe { parts } => {
                                    let submission = parts
                                        .iter()
                                        .map(|p| p.word.text.clone())
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    transcription_challenge::PartGraded::AskedToTranscribe {
                                        submission,
                                        parts: parts
                                            .into_iter()
                                            .map(|p| transcription_challenge::PartGradedPart {
                                                grade:
                                                    transcription_challenge::WordGrade::Perfect {
                                                        wrote: Some(p.word.text.clone()),
                                                    },
                                                heard: p,
                                            })
                                            .collect(),
                                    }
                                }
                                transcription_challenge::Part::Provided { part } => {
                                    transcription_challenge::PartGraded::Provided { part }
                                }
                            })
                            .collect();
                        self.deck.transcribe_sentence(graded)
                    }
                };

                if let Some(event) = event {
                    let ts = Timestamped {
                        timestamp: self.current_time,
                        within_device_events_index: self.event_index,
                        event,
                    };
                    self.deck = apply_event(self.deck, &ts);
                    self.event_index += 1;
                }
            } else {
                break;
            }
        }

        // Add 10 new cards at the end of the day
        if let Some(event) = self.deck.add_next_unknown_cards(None, 10, vec![]) {
            let ts = Timestamped {
                timestamp: self.current_time,
                within_device_events_index: self.event_index,
                event,
            };
            self.deck = apply_event(self.deck, &ts);
            self.event_index += 1;
        }

        // Advance to next day
        self.current_time += Duration::days(1);

        (self, day_challenges)
    }
}

impl Deck {
    /// Create an iterator that simulates daily usage starting from a specific time.
    /// The iterator yields all challenges for each day as a Vec, answering them perfectly,
    /// and adds 10 new cards at the end of each day.
    /// Use .take(n) to limit to n days.
    ///
    /// The start_time parameter ensures deterministic simulation -
    /// callers must be explicit about their time choice.
    pub fn simulate_usage(&self, start_time: DateTime<Utc>) -> DailySimulationIterator {
        DailySimulationIterator::new(self.clone(), start_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use language_utils::SentenceGram;
    use language_utils::language_pack::LanguagePack;
    use std::sync::Arc;

    fn load_language_pack(course: &language_utils::Course) -> Arc<LanguagePack> {
        let path = format!(
            "../out/{}_for_{}/language_data.rkyv",
            course.target_language.iso_639_3(),
            course.native_language.iso_639_3()
        );
        let bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
        let archived = rkyv::access::<
            language_utils::language_pack::ArchivedLanguagePack,
            rkyv::rancor::Error,
        >(&bytes)
        .unwrap();
        let language_pack: LanguagePack =
            rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived).unwrap();
        Arc::new(language_pack)
    }

    fn validate_language_pack(lp: &LanguagePack, course: &language_utils::Course) {
        let lang = course.target_language;
        let label = format!(
            "{} -> {}",
            course.native_language.iso_639_3(),
            course.target_language.iso_639_3()
        );

        // Every gram in gram_frequencies should have a definition
        for gram_spur in lp.gram_frequencies.keys() {
            assert!(
                lp.gram_definitions.contains_key(gram_spur),
                "[{label}] Gram {:?} is in gram_frequencies but has no definition",
                lp.gram_rodeo.resolve(gram_spur)
            );
        }

        // Every gram should produce a non-empty display string
        for gram_spur in lp.gram_frequencies.keys() {
            let resolved = lp.gram_rodeo.resolve(gram_spur).resolve(&lp.string_rodeo);
            let display = resolved.to_display_string(lang);
            assert!(
                !display.is_empty(),
                "[{label}] Gram {:?} produced an empty display string",
                lp.gram_rodeo.resolve(gram_spur)
            );
        }

        for (sentence_spur, sentence_grams) in &lp.encoded_sentences {
            // Every sentence should have at least one translation
            let translations = lp.translations.get(sentence_spur);
            assert!(
                translations.is_some_and(|t| !t.is_empty()),
                "[{label}] Sentence {:?} has no translations",
                lp.string_rodeo.resolve(sentence_spur)
            );

            // Every sentence should render to a non-empty sequence of literals
            let literals = lp.sentence_to_literals(sentence_spur, lang);
            assert!(
                literals.as_ref().is_some_and(|l| !l.is_empty()),
                "[{label}] Sentence {:?} produced no literals",
                lp.string_rodeo.resolve(sentence_spur)
            );

            // Every learnable gram in a sentence should have a definition
            for gram in &sentence_grams.grams {
                if let SentenceGram::Learnable(gram_spur) = gram {
                    assert!(
                        lp.gram_definitions.contains_key(gram_spur),
                        "[{label}] Learnable gram {:?} in sentence {:?} has no definition",
                        lp.gram_rodeo.resolve(gram_spur),
                        lp.string_rodeo.resolve(sentence_spur)
                    );
                }
            }
        }
    }

    #[test]
    fn test_simulate_365_days_default_deck_all_courses() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        for course in language_utils::COURSES {
            let language_pack = load_language_pack(course);
            validate_language_pack(&language_pack, course);

            let context = crate::Context {
                language_pack,
                course: course.clone(),
            };
            let state = crate::DeckState::new();
            let deck: Deck = <Deck as weapon::AppState>::finalize(state, &context);
            let mut simulator = deck.simulate_usage(fixed_time);

            for _ in 0..365 {
                let (next_sim, _challenges) = simulator.next();
                simulator = next_sim;
            }
        }
    }

    fn load_test_data_deck(language_pack: Arc<LanguagePack>) -> Deck {
        use std::collections::BTreeMap;
        use weapon::data_model::{EventStore, EventType, Timestamped};
        use weapon::opfs::parse_event_log_records;

        let mut store: EventStore<String, String> = EventStore::default();
        store.get_or_insert_default::<EventType<crate::DeckEvent>>(
            "reviews".to_string(),
            None,
        );

        let reviews_blob = std::fs::read(
            "test-data/.weapon/user-events/user__aa6b6044-10d0-444b-8518-3696a15d2392/stream__reviews/events.blob",
        )
        .expect("Failed to read reviews events blob");
        let review_records = parse_event_log_records(&reviews_blob);

        let mut reviews_by_device: BTreeMap<String, Vec<Timestamped<serde_json::Value>>> =
            BTreeMap::new();
        for record in &review_records {
            reviews_by_device
                .entry(record.device_id.clone())
                .or_default()
                .push(record.event.clone());
        }
        for (device_id, events) in reviews_by_device {
            store.add_device_events_jsons(
                "reviews".to_string(),
                device_id,
                events,
                None,
            );
        }

        let context = crate::Context {
            language_pack,
            course: language_utils::Course {
                target_language: language_utils::Language::French,
                native_language: language_utils::Language::English,
            },
        };
        let initial_state = crate::DeckState::new();
        let stream = store
            .get::<EventType<crate::DeckEvent>>("reviews".to_string())
            .expect("reviews stream should exist");
        stream.state(initial_state, &context)
    }

    #[test]
    fn test_simulate_365_days_test_data_deck() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        let course = language_utils::Course {
            target_language: language_utils::Language::French,
            native_language: language_utils::Language::English,
        };
        let language_pack = load_language_pack(&course);
        let deck = load_test_data_deck(language_pack);

        let mut simulator = deck.simulate_usage(fixed_time);
        for _ in 0..365 {
            let (next_sim, _challenges) = simulator.next();
            simulator = next_sim;
        }
    }

    #[test]
    fn test_simulator_is_deterministic() {
        // Create a fixed start time
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        // Run simulation 3 times and collect results
        let mut results = Vec::new();

        for _ in 0..3 {
            let deck = Deck::default();
            let mut simulator = deck.simulate_usage(fixed_time);

            // Collect challenges for first 5 days
            let mut challenges_per_day = Vec::new();
            for _ in 0..5 {
                let (next_sim, challenges) = simulator.next();
                simulator = next_sim;

                // Convert challenges to a comparable format (just count by type for simplicity)
                let mut flash_count = 0;
                let mut translate_count = 0;
                let mut transcribe_count = 0;

                for challenge in challenges {
                    match challenge {
                        Challenge::FlashCardReview { .. } => flash_count += 1,
                        Challenge::TranslateComprehensibleSentence(_) => translate_count += 1,
                        Challenge::TranscribeComprehensibleSentence(_) => transcribe_count += 1,
                    }
                }

                challenges_per_day.push((flash_count, translate_count, transcribe_count));
            }

            results.push(challenges_per_day);
        }

        // Verify all three runs produced identical results
        assert_eq!(
            results[0], results[1],
            "First and second simulation runs differ"
        );
        assert_eq!(
            results[1], results[2],
            "Second and third simulation runs differ"
        );
    }
}
