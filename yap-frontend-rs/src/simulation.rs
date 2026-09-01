use crate::Rating;
use crate::next_cards::AllowedCards;
use crate::{
    Challenge, Deck, DeckState, TranscribeComprehensibleSentence, TranslateComprehensibleSentence,
};
use chrono::{DateTime, Duration, Utc};
use language_utils::{Gram, transcription_challenge};
use wasm_bindgen::prelude::*;
use weapon::AppState;
use weapon::data_model::Timestamped;

fn apply_event(deck: Deck, event: &Timestamped<crate::DeckEvent>) -> Deck {
    let context = deck.context.clone();
    let state = DeckState::from(deck);
    let state = Deck::process_event(state, &context, event);
    Deck::finalize(state, &context)
}

/// Iterator that simulates daily usage of a deck.
/// Call `next_day()` to get a `DayChallengeIterator` for one day's challenges,
/// then call `finish_day()` on it to advance to the next day.
pub struct DailySimulationIterator {
    deck: Deck,
    current_time: DateTime<Utc>,
    event_index: usize,
    /// How many new cards to add at the end of each day. None uses the same
    /// smart-add batch size the app would offer (ramps up to ~10/day).
    new_cards_per_day: Option<usize>,
    /// Challenge types the user has turned off (can't listen / can't speak).
    banned_challenge_types: Vec<crate::ChallengeRequirements>,
    /// Whether locked cards are treated as due. Projections include them so
    /// long-range estimates aren't skewed by lockup; the audio prefetcher
    /// excludes them to match what the app will actually show.
    include_locked: bool,
}

impl DailySimulationIterator {
    pub fn new(deck: Deck, current_time: DateTime<Utc>) -> Self {
        Self {
            deck,
            current_time,
            event_index: 0,
            new_cards_per_day: None,
            banned_challenge_types: Vec::new(),
            include_locked: true,
        }
    }

    /// Override how many new cards are added per simulated day.
    pub fn with_new_cards_per_day(mut self, count: usize) -> Self {
        self.new_cards_per_day = Some(count);
        self
    }

    /// Restrict the simulation to challenges the app would actually show:
    /// exclude locked cards and respect the user's banned challenge types.
    /// Used by the audio prefetcher, where simulating a challenge the app
    /// will never display both wastes a fetch and lets cleanup delete clips
    /// that are genuinely upcoming.
    pub fn with_app_visible_challenges(
        mut self,
        banned_challenge_types: Vec<crate::ChallengeRequirements>,
    ) -> Self {
        self.banned_challenge_types = banned_challenge_types;
        self.include_locked = false;
        self
    }

    /// Start iterating over one day's challenges.
    /// Exhaust the returned iterator (or not), then call `finish_day()` to advance.
    pub fn next_day(self) -> DayChallengeIterator {
        DayChallengeIterator {
            deck: Some(self.deck),
            current_time: self.current_time,
            event_index: self.event_index,
            new_cards_per_day: self.new_cards_per_day,
            banned_challenge_types: self.banned_challenge_types,
            include_locked: self.include_locked,
            done: false,
        }
    }
}

/// Iterator over individual challenges within a single simulated day.
/// After consuming (or partially consuming) challenges, call `finish_day()`
/// to add new cards, advance time, and get back the `DailySimulationIterator`.
pub struct DayChallengeIterator {
    // Option used internally so we can temporarily take ownership in Iterator::next
    deck: Option<Deck>,
    current_time: DateTime<Utc>,
    event_index: usize,
    new_cards_per_day: Option<usize>,
    banned_challenge_types: Vec<crate::ChallengeRequirements>,
    include_locked: bool,
    done: bool,
}

impl DayChallengeIterator {
    fn deck(&self) -> &Deck {
        self.deck.as_ref().unwrap()
    }

    fn deck_mut(&mut self) -> &mut Deck {
        self.deck.as_mut().unwrap()
    }

    fn take_deck(&mut self) -> Deck {
        self.deck.take().unwrap()
    }

    /// Finish this day: add new cards, advance time, and return the simulation iterator.
    pub fn finish_day(mut self) -> DailySimulationIterator {
        let mut deck = self.take_deck();

        // Add new cards at the end of the day, drawn from the deck's active
        // sentence list (the AddCards event records the list, and replaying it
        // with None would reset the deck's selection).
        let sentence_list = deck.get_sentence_list();
        let event = match self.new_cards_per_day {
            Some(count) => {
                let cards: Vec<_> = deck
                    .next_unknown_cards(
                        AllowedCards::BannedRequirements(Default::default()),
                        &sentence_list,
                        count,
                    )
                    .take(count)
                    .collect();
                deck.cards_to_event(&cards, &sentence_list)
            }
            None => {
                deck.get_no_cards_ready_info(vec![], sentence_list)
                    .smart_add_event
            }
        };
        if let Some(event) = event {
            let ts = Timestamped {
                timestamp: self.current_time,
                within_device_events_index: self.event_index,
                timezone: Some(deck.context.timezone),
                event,
            };
            deck = apply_event(deck, &ts);
            self.event_index += 1;
        }

        DailySimulationIterator {
            deck,
            current_time: self.current_time + Duration::days(1),
            event_index: self.event_index,
            new_cards_per_day: self.new_cards_per_day,
            banned_challenge_types: self.banned_challenge_types,
            include_locked: self.include_locked,
        }
    }
}

impl Iterator for DayChallengeIterator {
    type Item = Challenge<Gram<String>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let review_info = self.deck().get_review_info_impl(
            self.banned_challenge_types.clone(),
            self.current_time.timestamp_millis() as f64,
            self.include_locked,
        );
        if let Some(challenge) = review_info.get_next_challenge(self.deck()) {
            let to_return = challenge.clone();
            // Answer the challenge, marking new flashcards as forgotten once
            let event = match challenge {
                Challenge::FlashCardReview {
                    indicator, is_new, ..
                }
                | Challenge::PronunciationChallenge {
                    indicator, is_new, ..
                } => {
                    let rating = if is_new {
                        Rating::Again
                    } else {
                        Rating::Remembered
                    };
                    self.deck_mut().review_card(indicator, rating)
                }
                Challenge::TranslateComprehensibleSentence(TranslateComprehensibleSentence {
                    target_language,
                    ..
                }) => self
                    .deck_mut()
                    .translate_sentence_perfect(vec![], target_language),
                Challenge::TranscribeComprehensibleSentence(TranscribeComprehensibleSentence {
                    parts,
                    ..
                }) => {
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
                                            grade: transcription_challenge::WordGrade::Perfect {
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
                    self.deck_mut().transcribe_sentence(graded)
                }
            };

            if let Some(event) = event {
                let ts = Timestamped {
                    timestamp: self.current_time,
                    within_device_events_index: self.event_index,
                    timezone: Some(self.deck().context.timezone),
                    event,
                };
                let deck = self.take_deck();
                self.deck = Some(apply_event(deck, &ts));
                self.event_index += 1;
            } else {
                // No event means the deck state didn't change (e.g. sentence
                // not found after cleanup). Stop to avoid an infinite loop.
                eprintln!(
                    "BUG: Simulation produced a challenge that returned no event: {to_return:?}"
                );
                #[cfg(debug_assertions)]
                panic!(
                    "Simulation produced a challenge that returned no event when answered — this would cause an infinite loop"
                );
                #[cfg(not(debug_assertions))]
                {
                    self.done = true;
                    return None;
                }
            }

            Some(to_return)
        } else {
            self.done = true;
            None
        }
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

/// One simulated day's results, sampled after the day's reviews and new cards.
#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SimulationSample {
    /// Days since the simulation started (1 = end of the first simulated day).
    pub day: u32,
    /// Percent known (0-100) of the active sentence list, if one is selected.
    pub goal_percent: Option<f64>,
    /// Percent of words known overall (0-100).
    pub overall_percent: f64,
    /// Challenges answered on this simulated day.
    pub reviews: u32,
    /// Total reviews on the deck by the end of this day (real history + simulated).
    pub total_reviews: u32,
    /// New cards added on this simulated day.
    pub new_cards: u32,
    /// Total cards on the deck by the end of this day.
    pub total_cards: u32,
}

/// A paused simulation of future deck usage. The frontend advances it in
/// chunks (e.g. two weeks at a time), yielding to the main thread between
/// chunks and rendering the samples incrementally.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct DeckSimulation {
    iterator: Option<DailySimulationIterator>,
    day: u32,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl DeckSimulation {
    /// Advance the simulation by `days` days, returning one sample per day.
    pub fn simulate_days(&mut self, days: u32) -> Vec<SimulationSample> {
        let Some(mut iterator) = self.iterator.take() else {
            return Vec::new();
        };

        let mut samples = Vec::with_capacity(days as usize);
        for _ in 0..days {
            let cards_before = iterator.deck.num_cards_added();
            let mut day_iter = iterator.next_day();
            let reviews = day_iter.by_ref().count() as u32;
            iterator = day_iter.finish_day();
            self.day += 1;

            let deck = &iterator.deck;
            let goal_percent = deck.get_sentence_list().map(|selection| {
                deck.sentence_list_percent_known(&Some(selection))
                    .percent_known
            });
            let total_cards = deck.num_cards_added();
            samples.push(SimulationSample {
                day: self.day,
                goal_percent,
                overall_percent: deck.get_percent_of_words_known() * 100.0,
                reviews,
                total_reviews: deck.get_total_reviews() as u32,
                new_cards: total_cards.saturating_sub(cards_before) as u32,
                total_cards: total_cards as u32,
            });
        }

        self.iterator = Some(iterator);
        samples
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Deck {
    /// Begin a chunked simulation of future usage starting at `timestamp_ms`,
    /// adding `new_cards_per_day` cards each simulated day (None uses the
    /// app's default smart-add batch size).
    /// The deck itself is not modified; the simulation runs on a clone.
    pub fn start_simulation(
        &self,
        timestamp_ms: f64,
        new_cards_per_day: Option<u32>,
    ) -> DeckSimulation {
        let start_time = DateTime::from_timestamp_millis(timestamp_ms as i64)
            .expect("invalid simulation start timestamp");
        let mut iterator = self.simulate_usage(start_time);
        if let Some(count) = new_cards_per_day {
            iterator = iterator.with_new_cards_per_day(count as usize);
        }
        DeckSimulation {
            iterator: Some(iterator),
            day: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SentenceListSelection;
    use crate::next_cards::AllowedCards;
    use chrono::TimeZone;
    use language_utils::SentenceGram;
    use language_utils::language_pack::LanguagePack;
    use std::sync::Arc;

    fn load_language_pack(course: &language_utils::Course) -> Arc<LanguagePack> {
        let dir = format!(
            "../out/{}_for_{}",
            course.target_language.code(),
            course.native_language.code()
        );
        let language_pack =
            language_utils::language_pack::load_split_dir(std::path::Path::new(&dir))
                .unwrap_or_else(|e| panic!("Failed to load language pack in {dir}: {e}"));
        Arc::new(language_pack)
    }

    fn validate_language_pack(lp: &LanguagePack, course: &language_utils::Course) {
        let lang = course.target_language;
        let label = format!(
            "{} -> {}",
            course.native_language.code(),
            course.target_language.code()
        );

        fn assert_frequency_list_sorted<'a>(
            lp: &LanguagePack,
            lang: language_utils::Language,
            label: &str,
            list_name: &str,
            entries: impl Iterator<Item = (&'a language_utils::SpurGram, &'a crate::Frequency)>,
        ) {
            let mut prev_count = u32::MAX;
            for (gram_spur, freq) in entries {
                assert!(
                    freq.count <= prev_count,
                    "[{label}] {list_name} not sorted by count descending: gram '{}' has count {} after count {}",
                    lp.gram_rodeo
                        .resolve(gram_spur)
                        .resolve(&lp.string_rodeo)
                        .to_display_string(lang),
                    freq.count,
                    prev_count
                );
                prev_count = freq.count;
            }
        }

        // Every gram in gram_frequencies should have a definition
        for gram_spur in lp.gram_frequencies.entries.keys() {
            let resolved = lp.gram_rodeo.resolve(gram_spur).resolve(&lp.string_rodeo);
            assert!(
                lp.gram_definitions.contains_key(gram_spur),
                "[{label}] Gram '{}' ({:?}) is in gram_frequencies but has no definition",
                resolved.to_display_string(lang),
                resolved
            );
        }

        // gram_frequencies should be sorted by count descending
        // (NextCardsIterator relies on this for early termination)
        assert_frequency_list_sorted(
            lp,
            lang,
            &label,
            "gram_frequencies",
            lp.gram_frequencies.entries.iter(),
        );
        for (source_id, freq_list) in &lp.source_gram_frequencies {
            let list_name = format!("source_gram_frequencies[{source_id:?}]");
            assert_frequency_list_sorted(lp, lang, &label, &list_name, freq_list.entries.iter());
        }

        // Every gram should produce a non-empty display string
        for gram_spur in lp.gram_frequencies.entries.keys() {
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
    #[ignore] // TODO: un-ignore once we regenerate more data
    fn test_simulate_365_days_default_deck_all_courses() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        for course in language_utils::COURSES {
            let language_pack = load_language_pack(course);
            validate_language_pack(&language_pack, course);

            let context = crate::Context {
                language_pack,
                course: *course,
                timezone: chrono::FixedOffset::east_opt(0).unwrap(),
            };
            let state = crate::DeckState::new();
            let deck: Deck = <Deck as weapon::AppState>::finalize(state, &context);
            let mut simulator = deck.simulate_usage(fixed_time);

            for _ in 0..365 {
                let day = simulator.next_day();
                // Exhaust challenges then advance
                simulator = day.finish_day();
            }
        }
    }

    fn load_test_data_deck(language_pack: Arc<LanguagePack>) -> Deck {
        use std::collections::BTreeMap;
        use weapon::data_model::{EventStore, EventType, Timestamped};
        use weapon::opfs::parse_event_log_records;

        let mut store: EventStore<String, String> = EventStore::default();
        store.get_or_insert_default::<EventType<crate::DeckEvent>>("reviews".to_string(), None);

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
            store.add_device_events_jsons("reviews".to_string(), device_id, events, None);
        }

        let context = crate::Context {
            language_pack,
            course: language_utils::Course {
                target_language: language_utils::Language::French,
                native_language: language_utils::Language::English,
            },
            timezone: chrono::FixedOffset::east_opt(0).unwrap(),
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
            let day = simulator.next_day();
            simulator = day.finish_day();
        }
    }

    /// Test that the early-termination optimization in NextCardsIterator produces
    /// exactly the same cards as a full evaluation of all grams.
    #[test]
    fn test_next_cards_early_termination_correctness() {
        let course = language_utils::Course {
            target_language: language_utils::Language::French,
            native_language: language_utils::Language::English,
        };
        let language_pack = load_language_pack(&course);
        let deck = load_test_data_deck(language_pack);

        let mut sentence_lists = vec![None];
        for source_id in deck.context.language_pack.source_gram_frequencies.keys() {
            let selection = match source_id {
                language_utils::FrequencySourceId::Movie(id) => {
                    SentenceListSelection::Movie { id: id.clone() }
                }
                language_utils::FrequencySourceId::PimsleurLesson(lesson) => {
                    SentenceListSelection::PimsleurLesson {
                        level: lesson.level,
                        lesson: lesson.lesson,
                    }
                }
            };
            sentence_lists.push(Some(selection));
        }

        for sentence_list in sentence_lists {
            for count in [1, 5, 10, 50, 100] {
                let result_a: Vec<_> = deck
                    .next_unknown_cards(
                        AllowedCards::BannedRequirements(Default::default()),
                        &sentence_list,
                        count,
                    )
                    .take(count)
                    .collect();

                let result_b: Vec<_> = deck
                    .next_unknown_cards(
                        AllowedCards::BannedRequirements(Default::default()),
                        &sentence_list,
                        usize::MAX / 4,
                    )
                    .take(count)
                    .collect();

                assert_eq!(
                    result_a, result_b,
                    "Results differ for sentence_list={sentence_list:?}, count={count}: early-terminated iterator produced different cards than unlimited iterator"
                );
            }
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
                let mut day = simulator.next_day();

                // Convert challenges to a comparable format (just count by type for simplicity)
                let mut flash_count = 0;
                let mut translate_count = 0;
                let mut transcribe_count = 0;

                for challenge in day.by_ref() {
                    match challenge {
                        Challenge::FlashCardReview { .. }
                        | Challenge::PronunciationChallenge { .. } => flash_count += 1,
                        Challenge::TranslateComprehensibleSentence(_) => translate_count += 1,
                        Challenge::TranscribeComprehensibleSentence(_) => transcribe_count += 1,
                    }
                }

                challenges_per_day.push((flash_count, translate_count, transcribe_count));
                simulator = day.finish_day();
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
