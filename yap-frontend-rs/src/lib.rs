#![deny(clippy::string_slice)]

mod audio;
mod challenge;
mod deck_event;
mod deck_selection;
mod dictionary;
mod directories;
mod language_pack;
mod next_cards;
mod notifications;
pub mod opfs_test;
mod placement_test;
pub mod profile;
pub mod simulation;
mod supabase;
mod utils;

pub use deck_event::*;

use language_utils::Atom;
use language_utils::HomophonePractice;
use language_utils::HomophoneSentencePair;
use language_utils::HomophoneWordPair;
use language_utils::ProperNounDefinition;
use language_utils::SentenceGrams;
use language_utils::SpurGram;
use language_utils::lowercase_first_letter;
pub use simulation::DailySimulationIterator;

use chrono::{DateTime, Utc};
use deck_selection::DeckSelectionEvent;
use futures::StreamExt;
use language_utils::Frequency;
use language_utils::Literal;
use language_utils::PartOfSpeech;
use language_utils::TtsProvider;
use language_utils::TtsRequest;
use language_utils::autograde;
use language_utils::features::{Morphology, WordPrefix};
use language_utils::language_pack::LanguagePack;
use language_utils::text_cleanup::{find_closest_match, normalize_for_grading};
use language_utils::transcription_challenge;
use language_utils::{Course, Language};
use language_utils::{
    Gram, GramDefinition, Heteronym, MovieMetadata, PronunciationGuide, SentenceGram, WordType,
};
use lasso::Spur;
use opfs::persistent::{self};
use pav_regression::{IsotonicRegression, Point, SmoothRegression};
use rs_fsrs::FSRS;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;
use std::sync::Arc;
use std::sync::LazyLock;
use wasm_bindgen::prelude::*;
use weapon::AppState as _;
use weapon::data_model::{EventStore, EventType, ListenerKey, Timestamped};

use crate::deck_selection::DeckSelection;
use crate::deck_selection::DeckSelectionPartial;
use crate::directories::Directories;
use crate::next_cards::AllowedCards;
use crate::utils::hit_ai_server;
use next_cards::NextCardsIterator;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_available_courses() -> Vec<language_utils::Course> {
    language_utils::COURSES.to_vec()
}

#[wasm_bindgen]
pub struct Weapon {
    // todo: move these into a type in `weapon`
    // btw, we should never hold a borrow across an .await. by avoiding this, we guarantee the absence of "borrow while locked" panics
    store: RefCell<EventStore<String, String>>,
    user_id: Option<String>,
    device_id: String,

    // not this ofc
    language_pack: RefCell<BTreeMap<Course, Arc<LanguagePack>>>,
    directories: Directories,
}

// putting this inside LOGGER prevents us from accidentally initializing the logger more than once
#[allow(clippy::declare_interior_mutable_const)]
const LOGGER: LazyLock<()> = LazyLock::new(|| {
    utils::set_panic_hook();

    wasm_logger::init(wasm_logger::Config::default());
    log::info!("Logging initialized");
});

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Weapon {
    // Todo: I want to mostly move this into `weapon`. The one holdup is that wasm-bindgen types can't be generic, necessitating wrappers
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub async fn new(
        user_id: Option<String>,
        sync_stream: js_sys::Function,
    ) -> Result<Self, persistent::Error> {
        // used to only initialize the logger once
        #[allow(clippy::borrow_interior_mutable_const)]
        *LOGGER;

        let directories = directories::get_directories(&user_id)
            .await
            .inspect_err(|e| {
                log::error!("Error getting directories: {e:?}");
            })?;

        if user_id.is_some() {
            EventStore::<String, String>::import_logged_out_user_data(
                directories.weapon_directory_handle.clone(),
                directories.user_events_directory_handle.clone(),
                &directories.current_user_directory_handle,
            )
            .await
            .inspect_err(|e| {
                log::error!("Error importing logged out data: {e:?}");
            })?;
        }

        let device_id =
            utils::get_or_create_device_id(&directories.weapon_directory_handle, &user_id)
                .await
                .inspect_err(|e| {
                    log::error!("Error getting device ID: {e:?}");
                })?;

        // should move this into a separate function
        let mut events: EventStore<String, String> = EventStore::default();

        events.register_listener(move |listener_id, stream_id| {
            #[cfg(target_arch = "wasm32")]
            {
                let this = JsValue::null();
                let listener_js: JsValue = listener_id.into();
                let stream_js = JsValue::from_str(&stream_id);
                let _ = sync_stream.call2(&this, &listener_js, &stream_js);
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = (listener_id, &sync_stream, stream_id);
            }
        });

        Ok(Self {
            store: RefCell::new(events),
            user_id,
            device_id,
            language_pack: RefCell::new(BTreeMap::new()),
            directories,
        })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn subscribe_to_stream(
        &self,
        stream_id: String,
        callback: js_sys::Function,
    ) -> ListenerKey {
        // After sync, flush any pending notifications to JS listeners
        let _flusher = FlushLater::new(self);

        self.store
            .borrow_mut()
            .register_listener(move |_, event_stream_id| {
                if event_stream_id == stream_id {
                    let this = JsValue::null();
                    let _ = callback.call0(&this);
                }
            })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn unsubscribe(&self, key: ListenerKey) {
        self.store.borrow_mut().unregister_listener(key)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn request_reviews(&self) {
        let _flusher = FlushLater::new(self); // The addition of a new stream can trigger listeners, so we want to make sure to flush them after.
        self.store
            .borrow_mut()
            .get_or_insert_default::<EventType<DeckEvent>>("reviews".to_string(), None);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn request_deck_selection(&self) {
        let _flusher = FlushLater::new(self); // The addition of a new stream can trigger listeners, so we want to make sure to flush them after.
        self.store
            .borrow_mut()
            .get_or_insert_default::<EventType<DeckSelectionEvent>>(
                "deck_selection".to_string(),
                None,
            );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_stream_num_events(&self, stream_id: String) -> Option<usize> {
        let store = self.store.borrow();
        if !store.loaded_at_least_once(&stream_id) {
            return None;
        }
        store.get_raw(stream_id.clone()).map(|s| s.num_events())
    }

    pub fn get_deck_selection_state(&self) -> Option<DeckSelection> {
        let store = self.store.borrow();
        store
            .get::<EventType<DeckSelectionEvent>>("deck_selection".to_string())
            .map(|s| {
                s.state(
                    DeckSelectionPartial {
                        target_language: None,
                        native_language: None,
                        starting_fresh: BTreeMap::new(),
                    },
                    &(),
                )
            })
    }

    pub async fn get_deck_state(
        &self,
        language_pack: FetchedLanguagePack,
        course: Course,
    ) -> Result<Deck, JsValue> {
        let language_pack = Arc::clone(&language_pack.pack);
        let target_language = course.target_language;
        let native_language = self
            .get_deck_selection_state()
            .and_then(|s| s.native_language)
            .unwrap_or(course.native_language);

        let context = Context {
            language_pack,
            course: Course {
                target_language,
                native_language,
            },
        };
        let initial_state = DeckState::new();
        let store = self.store.borrow_mut();
        let Some(stream) = store.get::<EventType<DeckEvent>>("reviews".to_string()) else {
            return Ok(Deck::finalize(initial_state, &context));
        };
        Ok(stream.state(initial_state, &context))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn sync_with_supabase(
        &self,
        access_token: String,
        modifier: Option<ListenerKey>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        if let Some(user_id) = &self.user_id {
            // After sync, flush any pending notifications to JS listeners
            let _flusher = FlushLater::new(self);

            EventStore::sync_with_supabase(
                &self.store,
                &access_token,
                supabase::supabase_config(),
                user_id,
                None,
                modifier,
            )
            .await?;
        }
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn sync(
        &self,
        stream_id: String,
        access_token: Option<String>,
        attempt_supabase: bool,
        modifier: Option<ListenerKey>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        // After sync, flush any pending notifications to JS listeners
        let _flusher = FlushLater::new(self);

        let is_initial_load = {
            let store = self.store.borrow();
            !store.loaded_at_least_once(&stream_id)
        };

        let start_time = if is_initial_load {
            web_sys::window()
                .and_then(|w| w.performance())
                .map(|p| p.now())
        } else {
            None
        };

        EventStore::load_from_local_storage(
            &self.store,
            &self.directories.current_user_directory_handle,
            stream_id.clone(),
            modifier,
        )
        .await?;

        if is_initial_load {
            if let (Some(start), Some(perf)) =
                (start_time, web_sys::window().and_then(|w| w.performance()))
            {
                log::info!(
                    "Initial load from disk for {stream_id} took {}ms",
                    perf.now() - start
                );
            }
        }

        {
            if self
                .store
                .borrow_mut()
                .mark_loaded(stream_id.clone(), modifier)
            {
                self.flush_notifications();
            }
        }

        EventStore::save_to_local_storage(
            &self.store,
            &self.directories.current_user_directory_handle,
            stream_id.clone(),
        )
        .await?;

        if attempt_supabase
            && let Some(access_token) = access_token
            && let Some(user_id) = &self.user_id
        {
            let supabase_sync_result = EventStore::sync_with_supabase(
                &self.store,
                &access_token,
                supabase::supabase_config(),
                user_id,
                Some(stream_id.clone()),
                modifier,
            )
            .await?;
            if supabase_sync_result.downloaded_from_supabase > 0 {
                EventStore::save_to_local_storage(
                    &self.store,
                    &self.directories.current_user_directory_handle,
                    stream_id,
                )
                .await?;
            }
        }

        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_timestamp_of_earliest_unsynced_event(
        &self,
        target: weapon::data_model::SyncTarget,
    ) -> Option<EarliestUnsyncedEvent> {
        self.store
            .borrow()
            .get_timestamp_of_earliest_unsynced_event(target)
            .map(|timestamp| EarliestUnsyncedEvent { timestamp })
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn load_from_local_storage(
        &self,
        stream_id: String,
    ) -> Result<(), persistent::Error> {
        let _flusher = FlushLater::new(self);

        EventStore::load_from_local_storage(
            &self.store,
            &self.directories.current_user_directory_handle,
            stream_id.clone(),
            None,
        )
        .await?;

        self.store.borrow_mut().mark_loaded(stream_id, None);

        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_sync_state(
        &self,
        target: weapon::data_model::SyncTarget,
    ) -> weapon::data_model::SyncState<String, String> {
        self.store
            .borrow()
            .sync_state(target)
            .cloned()
            .unwrap_or_default()
    }

    /// Flush pending store/stream notifications safely, avoiding RefCell re-borrows during callbacks.
    fn flush_notifications(&self) {
        // do it like this to avoid holding the borrow while we call the callbacks
        let notifications = self.store.borrow_mut().drain_due_notifications();
        // that's important because many of these callbacks will call back into rust functions that themselves do borrow_mut()
        for notification in notifications {
            notification();
        }
    }

    // =======
    // non-obviously for JS consumption
    // =======

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn num_events(&self) -> usize {
        self.store
            .borrow()
            .vector_clock()
            .values()
            .map(|device_counts| device_counts.values().sum::<usize>())
            .sum()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn num_events_on_remote_as_of_last_sync(
        &self,
        target: weapon::data_model::SyncTarget,
    ) -> usize {
        self.store
            .borrow()
            .sync_state(target)
            .map(|state| {
                state
                    .remote_clock
                    .values()
                    .map(|device_counts| device_counts.values().sum::<usize>())
                    .sum()
            })
            .unwrap_or(0)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn user_id(&self) -> Option<String> {
        self.user_id.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn device_id(&self) -> String {
        self.device_id.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn add_remote_event(
        &self,
        device_id: String,
        stream_id: String,
        event: String,
    ) -> Result<(), JsValue> {
        let event: serde_json::Value =
            serde_json::from_str(&event).map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        let versioned_event: Timestamped<EventType<VersionedDeckEvent>> =
            serde_json::from_value(event).map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        // Add the versioned event directly - it will be stored on disk.
        // Events that can't convert to current form will be skipped during state computation.
        self.store
            .borrow_mut()
            .add_device_event(stream_id, device_id, versioned_event, None);
        self.flush_notifications();
        Ok(())
    }

    // =======
    // less generic
    // =======-

    pub fn add_deck_event(&self, event: DeckEvent) {
        self.store.borrow_mut().add_raw_event(
            "reviews".to_string(),
            self.device_id.clone(),
            event,
            None,
        );
        self.flush_notifications();
    }

    pub fn add_deck_selection_event(&self, event: DeckSelectionEvent) {
        self.store.borrow_mut().add_raw_event(
            "deck_selection".to_string(),
            self.device_id.clone(),
            event,
            None,
        );
        self.flush_notifications();
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn cache_language_pack(&self, course: Course) {
        let _ = self.get_language_pack(course, None).await;
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct FetchedLanguagePack {
    pack: Arc<LanguagePack>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Weapon {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn get_language_pack(
        &self,
        course: Course,
        on_progress: Option<js_sys::Function>,
    ) -> Result<FetchedLanguagePack, language_pack::LanguageDataError> {
        let language_pack = if let Some(language_pack) = self.language_pack.borrow().get(&course) {
            language_pack.clone()
        } else {
            let language_pack = language_pack::get_language_pack(
                &self.directories.data_directory_handle,
                course,
                &|message: &str, progress: f32| {
                    if let Some(ref callback) = on_progress {
                        let this = wasm_bindgen::JsValue::NULL;
                        let message_js = wasm_bindgen::JsValue::from_str(message);
                        let progress_js = wasm_bindgen::JsValue::from_f64(progress as f64);
                        let _ = callback.call2(&this, &message_js, &progress_js);
                    }
                },
            )
            .await?;
            self.language_pack
                .borrow_mut()
                .insert(course, Arc::new(language_pack));

            self.language_pack
                .borrow()
                .get(&course)
                .expect("language pack must exist as we just added it")
                .clone()
        };
        Ok(FetchedLanguagePack {
            pack: language_pack,
        })
    }
}

#[derive(Clone, Debug, tsify::Tsify, serde::Serialize, serde::Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct EarliestUnsyncedEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// A simple struct that flushes event listeners when dropped. THis is useful if you want to ensure you don't forget to flush listeners, regardless of the code path a function takes.
struct FlushLater<'a> {
    weapon: &'a Weapon,
}

impl<'a> FlushLater<'a> {
    fn new(weapon: &'a Weapon) -> Self {
        Self { weapon }
    }
}

impl<'a> Drop for FlushLater<'a> {
    fn drop(&mut self) {
        self.weapon.flush_notifications();
    }
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TranslateComprehensibleSentence {
    pub audio: AudioRequest,
    pub target_language: String,
    pub target_language_literals: Vec<Literal<String>>,
    pub unique_target_language_phrases: Vec<String>,
    pub native_translations: Vec<String>,
    pub movie_titles: Vec<(String, String)>,
    pub proper_noun_definitions: Vec<(String, ProperNounDefinition)>,
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TranscribeComprehensibleSentence {
    pub target_language: String,
    pub audio: AudioRequest,
    pub native_language: String,
    pub parts: Vec<transcription_challenge::Part>,
    pub movie_titles: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PickHomophone<S>
where
    S: rkyv::Archive + Hash + std::fmt::Debug + Eq + PartialEq + Ord + PartialOrd,
    <S as rkyv::Archive>::Archived: PartialEq + PartialOrd + Eq + Ord + Hash + std::fmt::Debug,
{
    word_pair: HomophoneWordPair<S>,
    sentence_pair: HomophoneSentencePair<S>,
}

#[derive(
    Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify, Hash,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum CardType {
    TargetLanguage,
    Listening,
    LetterPronunciation,
}

const CARD_TYPES: [CardType; 3] = [
    CardType::TargetLanguage,
    CardType::Listening,
    CardType::LetterPronunciation,
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AddCardOptions {
    pub smart_add: u32,
    pub manual_add: Vec<(u32, CardType)>,
}

pub use deck_event::current::CardIndicator;

impl CardType {
    pub fn challenge_type(&self) -> ChallengeRequirements {
        match self {
            CardType::TargetLanguage => ChallengeRequirements::Text,
            CardType::Listening => ChallengeRequirements::Listening,
            CardType::LetterPronunciation => ChallengeRequirements::Speaking,
        }
    }
}

#[derive(Clone, Debug)]
enum CardStatus {
    Tracked(CardData),
    Unadded(Unadded),
}

impl CardStatus {
    pub(crate) fn is_new(&self) -> bool {
        match self {
            CardStatus::Tracked(CardData::Added { fsrs_card } | CardData::Ghost { fsrs_card }) => {
                fsrs_card.state == rs_fsrs::State::New
            }
            CardStatus::Unadded(_) => false,
        }
    }

    pub(crate) fn reviewed(&self) -> Option<&CardData> {
        match self {
            CardStatus::Tracked(card_data) => Some(card_data),
            CardStatus::Unadded(_) => None,
        }
    }

    pub(crate) fn unadded(&self) -> Option<&Unadded> {
        match self {
            CardStatus::Unadded(unadded) => Some(unadded),
            CardStatus::Tracked(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
struct Unadded {}

#[derive(Clone, Debug)]
enum CardData {
    /// Card that has been formally added to the deck
    Added { fsrs_card: rs_fsrs::Card },
    /// Ghost card - not formally added but has been reviewed through comprehensible sentences
    Ghost { fsrs_card: rs_fsrs::Card },
}

impl CardData {
    /// Returns positive surprise if there are no lapses, or negative surprise otherwise
    pub fn pre_existing_knowledge(&self) -> f64 {
        match self {
            CardData::Added { fsrs_card } | CardData::Ghost { fsrs_card } => {
                if fsrs_card.lapses == 0 {
                    fsrs_card.accumulated_positive_surprise
                } else {
                    -fsrs_card.accumulated_negative_surprise
                }
            }
        }
    }

    pub fn due_timestamp_ms(&self) -> f64 {
        match self {
            CardData::Added { fsrs_card } | CardData::Ghost { fsrs_card } => {
                fsrs_card.due.timestamp_millis() as f64
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct DailyStreak {
    streak_start: chrono::DateTime<chrono::Utc>,
    streak_expiry: chrono::DateTime<chrono::Utc>,
}

/// Context contains the language-specific configuration
#[derive(Clone, Debug)]
pub struct Context {
    pub language_pack: Arc<LanguagePack>,
    pub course: Course,
}

/// Flashcard types for tracking tutorial progress
#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum FlashcardType {
    WrittenGram,
    Listening,
    LetterPronunciation,
}

/// Stats contains review statistics and progress tracking
#[derive(Clone, Debug)]
pub struct Stats {
    pub sentences_reviewed: BTreeMap<Spur, u32>,
    pub words_listened_to: BTreeMap<Heteronym<Spur>, u32>,
    pub sentence_pairs_reviewed: BTreeMap<HomophoneSentencePair<Spur>, u32>,
    pub total_reviews: u64,
    pub xp: f64,
    pub daily_streak: Option<DailyStreak>,
    /// Track daily challenge completions for the past week
    /// Key is days since epoch, value is number of challenges completed
    pub past_week_challenges: BTreeMap<i64, u32>,
    /// Timestamp of the first event processed (when the user started using the app)
    pub start_time: Option<DateTime<Utc>>,
    /// Track how many times each flashcard type has been seen (for tutorial purposes)
    pub flashcard_type_seen_count: BTreeMap<FlashcardType, u32>,
}

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize, tsify::Tsify,
)]
#[serde(tag = "type")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum UserStatedExperience {
    PlacementTest { results: PlacementTest },
    FreshStart {},
}

#[derive(Clone, Debug)]
pub struct DeckState {
    placement_test_results: Option<PlacementTest>,
    cards: FxHashMap<CardIndicator<SpurGram, Spur>, CardData>,
    fsrs: FSRS,
    stats: Stats,
    /// Maps cards that have been detected as leeches to the total_reviews count when detected
    leeches: BTreeMap<CardIndicator<SpurGram, Spur>, u64>,
}

#[derive(Clone, Debug)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Deck {
    placement_test_results: Option<PlacementTest>,
    cards: FxHashMap<CardIndicator<SpurGram, Spur>, CardStatus>,
    fsrs: FSRS,
    pub(crate) stats: Stats,
    pub(crate) context: Context,
    regressions: Regressions,
    /// Maps cards that have been detected as leeches to the total_reviews count when detected
    leeches: BTreeMap<CardIndicator<SpurGram, Spur>, u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct Regressions {
    target_language_regression: Option<SmoothRegression<f64>>,
    listening_regression: Option<SmoothRegression<f64>>,
}

struct ComprehensibleSentence {
    target_language: Spur,
    target_language_sentence_grams: SentenceGrams<SpurGram>,
    unique_target_language_phrases: Vec<SpurGram>,
    native_languages: Vec<Spur>,
}

impl From<Deck> for DeckState {
    fn from(deck: Deck) -> Self {
        // Convert cards from CardStatus to CardData, only keeping Added cards
        let cards = deck
            .cards
            .iter()
            .filter_map(|(indicator, status)| match status {
                CardStatus::Tracked(data) => Some((*indicator, data.clone())),
                CardStatus::Unadded { .. } => None,
            })
            .collect();

        DeckState {
            placement_test_results: deck.placement_test_results,
            cards,
            fsrs: deck.fsrs,
            stats: deck.stats,
            leeches: deck.leeches,
        }
    }
}

impl weapon::AppState for Deck {
    type Event = DeckEvent;
    type Partial = DeckState;

    fn process_event(
        mut deck: Self::Partial,
        context: &<Self::Event as weapon::data_model::Event>::Context,
        event: &Timestamped<Self::Event>,
    ) -> Self::Partial {
        let Timestamped::<DeckEvent> {
            event,
            timestamp,
            within_device_events_index: _,
        } = event;

        let DeckEvent::Language(LanguageEvent {
            target_language: event_language,
            native_language: _, // TODO: specify native_language
            content: event,
        }) = event;

        // Set start_time on first event
        if deck.stats.start_time.is_none() {
            deck.stats.start_time = Some(*timestamp);
        }

        deck.update_daily_streak(timestamp);
        deck.stats.total_reviews += 1;

        // Clean up leeches that are more than 250 reviews old
        let current_reviews = deck.stats.total_reviews;
        deck.leeches
            .retain(|_, detected_at| current_reviews - *detected_at <= 250);

        if *event_language != context.course.target_language {
            return deck;
        }

        // Track challenge completions for workload statistics
        match event {
            LanguageEventContent::TranslationChallenge { .. }
            | LanguageEventContent::TranscriptionChallenge { .. } => {
                let days_since_epoch = timestamp.timestamp() / 86400;
                *deck
                    .stats
                    .past_week_challenges
                    .entry(days_since_epoch)
                    .or_insert(0) += 1;

                // Clean up old entries (keep only last 7 days)
                let seven_days_ago = days_since_epoch - 7;
                deck.stats
                    .past_week_challenges
                    .retain(|&day, _| day > seven_days_ago);
            }
            _ => {}
        }

        match event {
            LanguageEventContent::CompletePlacementTest { results } => {
                deck.placement_test_results = Some(results.clone());
            }
            LanguageEventContent::AddCards { cards } => {
                for (index, card) in cards.iter().enumerate() {
                    if let Some(card) = card.get_interned(
                        &context.language_pack.string_rodeo,
                        &context.language_pack.gram_rodeo,
                    ) {
                        // Make sure the card is valid and can be added
                        if !context.is_card_valid(&card) {
                            continue;
                        }
                        // Add the card to the deck if it's not already in it, or transition ghost to added
                        deck.cards
                            .entry(card)
                            .and_modify(|existing| {
                                // If it's a ghost card, transition it to added
                                if let CardData::Ghost { fsrs_card } = existing {
                                    let mut new_fsrs_card = fsrs_card.clone();
                                    // Reset the due date to now when formally adding
                                    new_fsrs_card.due = *timestamp;
                                    *existing = CardData::Added {
                                        fsrs_card: new_fsrs_card,
                                    };
                                }
                            })
                            .or_insert_with(|| {
                                let fsrs_card = rs_fsrs::Card::new(
                                    *timestamp + chrono::Duration::milliseconds(index as i64),
                                );
                                CardData::Added { fsrs_card }
                            });
                    }
                }
            }
            LanguageEventContent::ReviewCard { reviewed, rating } => {
                if let Some(reviewed) = reviewed.get_interned(
                    &context.language_pack.string_rodeo,
                    &context.language_pack.gram_rodeo,
                ) {
                    // Track flashcard type for tutorial purposes
                    if let Some(flashcard_type) = reviewed.get_flashcard_type() {
                        *deck
                            .stats
                            .flashcard_type_seen_count
                            .entry(flashcard_type)
                            .or_insert(0) += 1;
                    }
                    deck.log_review(reviewed, *rating, *timestamp, context);
                }
            }
            LanguageEventContent::TranslationChallenge { review, legacy } => {
                // Status: (hinted, remembered)
                type TranslationStatus = (bool, Option<bool>);

                // Extract literals into (Spur, status) pairs
                let literals: Vec<(Literal<Spur>, TranslationStatus)> = {
                    let mut statuses: Vec<_> = match &review {
                        current::SentenceReviewResult::Perfect { literals, .. } => literals
                            .iter()
                            .map(|(literal, hinted)| {
                                let hinted = hinted.unwrap_or(false);
                                (literal.clone(), (hinted, Some(true)))
                            })
                            .collect(),
                        current::SentenceReviewResult::Graded { literals, .. } => literals
                            .iter()
                            .map(|(literal, result)| match result {
                                None => (literal.clone(), (false, None)),
                                Some(learnable) => {
                                    (literal.clone(), (learnable.hinted, learnable.remembered))
                                }
                            })
                            .collect(),
                    };
                    // Lowercase the first letter to match encoded grams
                    if let Some((first_literal, _)) = statuses.first_mut() {
                        first_literal.word.text =
                            lowercase_first_letter(&first_literal.word.text).0;
                    }
                    // Intern all texts, skipping any not found
                    statuses
                        .into_iter()
                        .filter_map(|(literal, status)| {
                            match literal.get_interned(&context.language_pack.string_rodeo) {
                                Some(spur) => Some((spur, status)),
                                None => {
                                    log::warn!("Literal text not found in rodeo: {literal:?}");
                                    None
                                }
                            }
                        })
                        .collect()
                };

                // Get the challenge sentence
                let challenge_sentence = match &review {
                    current::SentenceReviewResult::Perfect { challenge, .. } => challenge,
                    current::SentenceReviewResult::Graded { challenge, .. } => challenge,
                };

                // Clean the sentence before lookup
                let cleaned_sentence = language_utils::text_cleanup::cleanup_sentence(
                    challenge_sentence.clone(),
                    context.course.target_language,
                );

                if let Some(sentence_spur) =
                    context.language_pack.string_rodeo.get(&cleaned_sentence)
                    && let Some(encoded_sentence) =
                        context.language_pack.encoded_sentences.get(&sentence_spur)
                {
                    // Update sentence review count
                    *deck
                        .stats
                        .sentences_reviewed
                        .entry(sentence_spur)
                        .or_insert(0) += 1;

                    let mut remembered_grams = BTreeSet::new();
                    let mut forgotten_grams = BTreeSet::new();

                    // Phrases are now unified as grams - legacy multiword terms get interned into gram_rodeo

                    // Match grams to literals and log reviews for multi-word grams
                    for (gram, matched) in utils::match_grams_to_literals(
                        encoded_sentence,
                        &literals,
                        &context.language_pack,
                    ) {
                        let any_hinted = matched.iter().any(|(_, (h, _))| *h);
                        let any_forgotten = matched.iter().any(|(_, (_, r))| *r == Some(false));
                        let all_remembered = matched.iter().all(|(_, (_, r))| *r == Some(true));

                        if any_forgotten || any_hinted {
                            forgotten_grams.insert(gram);
                        } else if all_remembered {
                            remembered_grams.insert(gram);
                        }

                        let gram = context.language_pack.gram_rodeo.resolve(&gram);
                        if gram.len() > 1 {
                            for (literal, (hinted, remembered)) in matched {
                                let language_utils::WordType::Heteronym(_) =
                                    &literal.word.word_type
                                else {
                                    continue;
                                };

                                let gram = Gram(vec![Atom::Tok(literal.word)]);
                                if let Some(gram) = context.language_pack.gram_rodeo.get(&gram) {
                                    if *hinted || *remembered == Some(false) {
                                        forgotten_grams.insert(gram);
                                    } else if *remembered == Some(true) {
                                        remembered_grams.insert(gram);
                                    }
                                }
                            }
                        }
                        // else: Unknown state, skip this gram
                    }

                    // Handle phrases (multiword terms) via remembered_grams/forgotten_grams
                    match &review {
                        current::SentenceReviewResult::Perfect { .. } => {
                            for gram_spur in encoded_sentence
                                .multiword_terms
                                .iter()
                                .chain(encoded_sentence.low_confidence_multiword_terms.iter())
                            {
                                remembered_grams.insert(*gram_spur);
                            }
                        }
                        current::SentenceReviewResult::Graded { phrases, .. } => {
                            for (phrase, remembered) in phrases {
                                let matching_gram = encoded_sentence
                                    .multiword_terms
                                    .iter()
                                    .chain(encoded_sentence.low_confidence_multiword_terms.iter())
                                    .find(|gram_spur| {
                                        let resolved = context
                                            .language_pack
                                            .gram_rodeo
                                            .resolve(gram_spur)
                                            .resolve(&context.language_pack.string_rodeo);
                                        let display = resolved
                                            .to_display_string(context.course.target_language);
                                        display == *phrase
                                    });
                                if let Some(gram_spur) = matching_gram {
                                    match remembered {
                                        Some(true) => {
                                            remembered_grams.insert(*gram_spur);
                                        }
                                        Some(false) => {
                                            forgotten_grams.insert(*gram_spur);
                                        }
                                        None => {}
                                    }
                                }
                            }
                        }
                    }

                    {
                        for lexeme in &legacy.lexemes_remembered {
                            match lexeme {
                                language_utils::Lexeme::Heteronym { heteronym } => {
                                    let Some(heteronym) =
                                        heteronym.get_interned(&context.language_pack.string_rodeo)
                                    else {
                                        continue;
                                    };
                                    let Some(grams) =
                                        context.language_pack.heteronym_to_grams.get(&heteronym)
                                    else {
                                        continue;
                                    };
                                    let Some(gram) = grams.first() else {
                                        continue;
                                    };
                                    remembered_grams.insert(*gram);
                                }
                                language_utils::Lexeme::Multiword { phrase } => {
                                    if let Some(grams) =
                                        context.language_pack.string_to_grams.get(phrase)
                                    {
                                        if let Some(gram) = grams.first() {
                                            remembered_grams.insert(*gram);
                                        }
                                    }
                                }
                            }
                        }
                        for lexeme in &legacy.lexemes_forgotten {
                            match lexeme {
                                language_utils::Lexeme::Heteronym { heteronym } => {
                                    let Some(heteronym) =
                                        heteronym.get_interned(&context.language_pack.string_rodeo)
                                    else {
                                        continue;
                                    };
                                    let Some(grams) =
                                        context.language_pack.heteronym_to_grams.get(&heteronym)
                                    else {
                                        continue;
                                    };
                                    let Some(gram) = grams.first() else {
                                        continue;
                                    };
                                    forgotten_grams.insert(*gram);
                                }
                                language_utils::Lexeme::Multiword { phrase } => {
                                    if let Some(grams) =
                                        context.language_pack.string_to_grams.get(phrase)
                                    {
                                        if let Some(gram) = grams.first() {
                                            forgotten_grams.insert(*gram);
                                        }
                                    }
                                }
                            }
                        }
                        for heteronym in &legacy.heteronyms_needed_hint {
                            let Some(heteronym) =
                                heteronym.get_interned(&context.language_pack.string_rodeo)
                            else {
                                continue;
                            };
                            let Some(grams) =
                                context.language_pack.heteronym_to_grams.get(&heteronym)
                            else {
                                continue;
                            };
                            let Some(gram) = grams.first() else {
                                continue;
                            };
                            forgotten_grams.insert(*gram);
                        }
                    }

                    for gram in remembered_grams.difference(&forgotten_grams) {
                        let card = CardIndicator::WrittenGram { gram: *gram };
                        if context.is_card_valid(&card) {
                            deck.log_review(card, current::Rating::Remembered, *timestamp, context);
                        }
                    }
                    for gram in &forgotten_grams {
                        let card = CardIndicator::WrittenGram { gram: *gram };
                        if context.is_card_valid(&card) {
                            deck.log_review(card, current::Rating::Again, *timestamp, context);
                        }
                    }
                }
            }
            LanguageEventContent::TranscriptionChallenge { challenge } => {
                // Extract literals with grades as (Spur, WordGrade) pairs
                let literals: Vec<(Literal<Spur>, transcription_challenge::WordGrade)> = {
                    let mut grades: Vec<_> = challenge
                        .iter()
                        .flat_map(|part| match part {
                            transcription_challenge::PartGraded::AskedToTranscribe {
                                parts,
                                ..
                            } => parts
                                .iter()
                                .map(|p| (p.heard.clone(), Some(p.grade.clone())))
                                .collect::<Vec<_>>(),
                            transcription_challenge::PartGraded::Provided { part } => {
                                // Provided parts (punctuation) don't have grades
                                vec![(part.clone(), None)]
                            }
                        })
                        .collect();

                    // Lowercase the first letter to match encoded grams
                    if let Some((first_literal, _)) = grades.first_mut() {
                        first_literal.word.text =
                            lowercase_first_letter(&first_literal.word.text).0;
                    }

                    // Intern all texts and filter to only graded parts
                    grades
                        .into_iter()
                        .filter_map(|(literal, grade)| {
                            let grade = grade?; // Skip provided parts without grades
                            match literal.get_interned(&context.language_pack.string_rodeo) {
                                Some(spur) => Some((spur, grade)),
                                None => {
                                    log::warn!(
                                        "Transcription literal text not found in rodeo: {literal:?}"
                                    );
                                    None
                                }
                            }
                        })
                        .collect()
                };

                // Reconstruct the challenge sentence for lookup
                let challenge_sentence: String = challenge
                    .iter()
                    .flat_map(|part| match part {
                        transcription_challenge::PartGraded::AskedToTranscribe {
                            parts, ..
                        } => parts
                            .iter()
                            .flat_map(|part| {
                                vec![part.heard.word.text.clone(), part.heard.whitespace.clone()]
                            })
                            .collect::<Vec<_>>(),
                        transcription_challenge::PartGraded::Provided { part } => {
                            vec![part.word.text.clone(), part.whitespace.clone()]
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("");

                // Clean the sentence before lookup (e.g., French punctuation spacing)
                let cleaned_sentence = language_utils::text_cleanup::cleanup_sentence(
                    challenge_sentence,
                    context.course.target_language,
                );

                if let Some(sentence_spur) =
                    context.language_pack.string_rodeo.get(&cleaned_sentence)
                {
                    let mut any_again = false;

                    let encoded_sentence =
                        context.language_pack.encoded_sentences.get(&sentence_spur);
                    if let Some(encoded_sentence) = encoded_sentence {
                        // Collect grams by rating (worse ratings take precedence)
                        let mut again_grams = BTreeSet::new();
                        let mut hard_grams = BTreeSet::new();
                        let mut remembered_grams = BTreeSet::new();

                        // Match grams to literals and categorize by rating
                        for (gram, matched) in utils::match_grams_to_literals(
                            encoded_sentence,
                            &literals,
                            &context.language_pack,
                        ) {
                            // Find worst grade (WordGrade is Ord: worse > better)
                            let worst_grade = matched.iter().max();

                            if let Some((_, grade)) = worst_grade {
                                match grade {
                                    transcription_challenge::WordGrade::Perfect { .. }
                                    | transcription_challenge::WordGrade::CorrectWithTypo { .. } => {
                                        remembered_grams.insert(gram);
                                    }
                                    transcription_challenge::WordGrade::PhoneticallyIdenticalButContextuallyIncorrect { .. } => {
                                        hard_grams.insert(gram);
                                    }
                                    _ => {
                                        again_grams.insert(gram);
                                    }
                                };
                            }

                            // Also categorize individual words from multi-word grams
                            let resolved_gram = context.language_pack.gram_rodeo.resolve(&gram);
                            if resolved_gram.len() > 1 {
                                for (literal, grade) in matched {
                                    let language_utils::WordType::Heteronym(_) =
                                        &literal.word.word_type
                                    else {
                                        continue;
                                    };

                                    let word_gram = Gram(vec![Atom::Tok(literal.word)]);
                                    if let Some(word_gram) =
                                        context.language_pack.gram_rodeo.get(&word_gram)
                                    {
                                        match grade {
                                            transcription_challenge::WordGrade::Perfect { .. }
                                            | transcription_challenge::WordGrade::CorrectWithTypo { .. } => {
                                                remembered_grams.insert(word_gram);
                                            }
                                            transcription_challenge::WordGrade::PhoneticallyIdenticalButContextuallyIncorrect { .. } => {
                                                hard_grams.insert(word_gram);
                                            }
                                            _ => {
                                                again_grams.insert(word_gram);
                                            }
                                        };
                                    }
                                }
                            }
                        }

                        // Log reviews with proper precedence (again > hard > remembered)
                        any_again = !again_grams.is_empty();

                        for gram in &again_grams {
                            deck.log_review(
                                current::CardIndicator::ListeningGram { gram: *gram },
                                current::Rating::Again,
                                *timestamp,
                                context,
                            );
                        }
                        for gram in hard_grams.difference(&again_grams) {
                            deck.log_review(
                                current::CardIndicator::ListeningGram { gram: *gram },
                                current::Rating::Hard,
                                *timestamp,
                                context,
                            );
                        }
                        for gram in remembered_grams
                            .difference(&again_grams)
                            .copied()
                            .collect::<BTreeSet<_>>()
                            .difference(&hard_grams)
                        {
                            deck.log_review(
                                current::CardIndicator::ListeningGram { gram: *gram },
                                current::Rating::Remembered,
                                *timestamp,
                                context,
                            );
                        }
                    }
                    // Update sentence review count if perfect (no Again ratings)
                    if !any_again {
                        *deck
                            .stats
                            .sentences_reviewed
                            .entry(sentence_spur)
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        deck
    }

    fn finalize(
        state: Self::Partial,
        context: &<Self::Event as weapon::data_model::Event>::Context,
    ) -> Self {
        // Collect data points for isotonic regression
        let mut target_language_points = Vec::new();
        let mut listening_points = Vec::new();

        for (card_indicator, card_data) in state.cards.iter() {
            // Only use cards that have been reviewed (not new)
            // For regression, only use Added cards that aren't new
            match card_data {
                CardData::Added { fsrs_card } | CardData::Ghost { fsrs_card }
                    if fsrs_card.state == rs_fsrs::State::New =>
                {
                    continue;
                }
                _ => {}
            }

            if let Some(frequency) = context.get_card_frequency(card_indicator) {
                let pre_existing_knowledge = card_data.pre_existing_knowledge();
                let point = Point::new(frequency.ln_frequency(), pre_existing_knowledge);

                match card_indicator {
                    CardIndicator::WrittenGram { .. } => {
                        target_language_points.push(point);
                    }
                    CardIndicator::ListeningGram { .. } => {
                        listening_points.push(point);
                    }
                    CardIndicator::LetterPronunciation { .. } => {}
                }
            }
        }

        // Add bias points at (0, -10) and (10, -10) to ensure the curve slopes down
        // This represents a word with 0 occurrences being very difficult. We'll give them a weight of 10 to ensure it's not ignored

        let bias_points = if let Some(placement_test_results) = &state.placement_test_results {
            // Use placement test results to create bias points
            let mut points = context.get_placement_test_points(placement_test_results);
            points.extend_from_slice(&[
                Point::new_with_weight(Frequency { count: 1 }.ln_frequency(), -10.0, 5.0),
                Point::new_with_weight(Frequency { count: 25 }.ln_frequency(), 0.0, 5.0),
                Point::new_with_weight(Frequency { count: 64 }.ln_frequency(), 0.0, 5.0),
            ]);
            points
        } else {
            vec![
                Point::new_with_weight(Frequency { count: 1 }.ln_frequency(), -10.0, 5.0),
                Point::new_with_weight(Frequency { count: 25 }.ln_frequency(), 0.0, 5.0),
                Point::new_with_weight(Frequency { count: 64 }.ln_frequency(), 0.0, 5.0),
                Point::new_with_weight(Frequency { count: 400 }.ln_frequency(), 0.0, 3.0),
                Point::new_with_weight(Frequency { count: 800 }.ln_frequency(), 0.0, 3.0),
                Point::new_with_weight(Frequency { count: 1000 }.ln_frequency(), 0.0, 3.0),
                Point::new_with_weight(Frequency { count: 1500 }.ln_frequency(), 0.0, 3.0),
                Point::new_with_weight(Frequency { count: 2000 }.ln_frequency(), 0.0, 2.0),
                Point::new_with_weight(Frequency { count: 2500 }.ln_frequency(), 0.0, 2.0),
                Point::new_with_weight(Frequency { count: 3000 }.ln_frequency(), 0.0, 2.0),
                Point::new_with_weight(Frequency { count: 3500 }.ln_frequency(), 0.0, 2.0),
                Point::new_with_weight(Frequency { count: 4000 }.ln_frequency(), 0.0, 2.0),
            ]
        };

        // Calculate smoothing window as 20% of max ln_frequency
        let smoothing_window = context
            .language_pack
            .gram_frequencies
            .get_index(0)
            .map(|(_, freq)| freq.ln_frequency() * 0.2)
            .unwrap_or(1.0); // Fallback if no frequencies exist

        // Create isotonic regressions (need at least 2 non-new cards)
        let target_language_regression =
            if target_language_points.len() >= 2 || state.placement_test_results.is_some() {
                target_language_points.extend_from_slice(&bias_points[..]);
                IsotonicRegression::new_ascending(&target_language_points)
                    .inspect_err(|e| log::error!("regression error: {e:?}"))
                    .ok()
                    .map(|reg| SmoothRegression::from_regression(&reg, smoothing_window))
            } else {
                None
            };

        let listening_regression =
            if listening_points.len() >= 2 || state.placement_test_results.is_some() {
                listening_points.extend_from_slice(&bias_points);
                IsotonicRegression::new_ascending(&listening_points)
                    .inspect_err(|e| log::error!("regression error: {e:?}"))
                    .ok()
                    .map(|reg| SmoothRegression::from_regression(&reg, smoothing_window))
            } else {
                None
            };

        let regressions = Regressions {
            target_language_regression,
            listening_regression,
        };

        // Convert existing cards to CardStatus and calculate probabilities for unadded cards
        let added_cards: FxHashMap<CardIndicator<SpurGram, Spur>, CardData> = state.cards;

        // Create all cards as Unadded first, then update with Added status
        let mut all_cards: FxHashMap<CardIndicator<SpurGram, Spur>, CardStatus> = context
            .language_pack
            .gram_frequencies
            .keys()
            .map(|gram| {
                (
                    CardIndicator::WrittenGram { gram: *gram },
                    CardStatus::Unadded(Unadded {}),
                )
            })
            .chain(context.language_pack.gram_frequencies.keys().map(|gram| {
                (
                    CardIndicator::ListeningGram { gram: *gram },
                    CardStatus::Unadded(Unadded {}),
                )
            }))
            .chain(
                // Add pronunciation pattern cards
                context
                    .language_pack
                    .pronunciation_data
                    .guides
                    .iter()
                    .filter_map(|guide| {
                        // Only create cards for patterns that exist in the rodeo
                        context
                            .language_pack
                            .string_rodeo
                            .get(&guide.pattern)
                            .map(|pattern| {
                                (
                                    CardIndicator::LetterPronunciation {
                                        pattern,
                                        position: guide.position,
                                    },
                                    CardStatus::Unadded(Unadded {}),
                                )
                            })
                    }),
            )
            .collect();

        // Update the cards that have been added
        for (indicator, card_data) in added_cards {
            all_cards.insert(indicator, CardStatus::Tracked(card_data));
        }

        Deck {
            placement_test_results: state.placement_test_results,
            cards: all_cards,
            fsrs: state.fsrs,
            stats: state.stats,
            context: context.clone(),
            regressions,
            leeches: state.leeches,
        }
    }
}

impl Default for DeckState {
    fn default() -> Self {
        Self::new()
    }
}

impl DeckState {
    /// Create a new empty DeckState
    pub fn new() -> Self {
        Self {
            placement_test_results: None,
            cards: FxHashMap::default(),
            fsrs: FSRS::new(rs_fsrs::Parameters {
                request_retention: 0.7,
                ..Default::default()
            }),
            stats: Stats {
                sentences_reviewed: BTreeMap::new(),
                words_listened_to: BTreeMap::new(),
                sentence_pairs_reviewed: BTreeMap::new(),
                total_reviews: 0,
                xp: 0.0,
                daily_streak: None,
                past_week_challenges: BTreeMap::new(),
                start_time: None,
                flashcard_type_seen_count: BTreeMap::new(),
            },
            leeches: BTreeMap::new(),
        }
    }

    fn log_review(
        &mut self,
        card: CardIndicator<SpurGram, Spur>,
        rating: Rating,
        timestamp: DateTime<Utc>,
        context: &Context,
    ) {
        // Make sure the card is valid before logging a review
        if !context.is_card_valid(&card) {
            return;
        }

        let card_data = self.cards.entry(card).or_insert_with(|| {
            // Create a ghost card if it doesn't exist
            let mut fsrs_card = rs_fsrs::Card::new(timestamp);
            fsrs_card.due = timestamp;
            CardData::Ghost { fsrs_card }
        });

        // Update the card data
        let fsrs_card = match card_data {
            CardData::Added { fsrs_card } | CardData::Ghost { fsrs_card } => fsrs_card,
        };
        let fsrs_rating = match rating {
            Rating::Again => rs_fsrs::Rating::Again,
            Rating::Remembered => {
                // for new cards, we use Easy. Otherwise, we use Good
                if fsrs_card.state == rs_fsrs::State::New {
                    rs_fsrs::Rating::Easy
                } else {
                    rs_fsrs::Rating::Good
                }
            }
            Rating::Hard => rs_fsrs::Rating::Hard,
            Rating::Good => rs_fsrs::Rating::Good,
            Rating::Easy => rs_fsrs::Rating::Easy,
        };

        *fsrs_card = self
            .fsrs
            .next(fsrs_card.clone(), timestamp, fsrs_rating)
            .card;

        // Detect leeches: cards with high lapse rate
        // Require at least 8 reviews to avoid false positives early on
        // A card is a leech if 40% or more of its reviews are lapses
        if fsrs_card.lapses >= 12 && fsrs_card.lapses % 4 == 0 {
            let lapse_ratio = fsrs_card.lapses as f64 / fsrs_card.reps as f64;
            if lapse_ratio >= 0.3 {
                // Mark as leech and reset to New state
                // This prevents it from being considered known for the purposes of challenge sentence selection
                self.leeches.insert(card, self.stats.total_reviews);
                fsrs_card.state = rs_fsrs::State::New;
            }
        }

        // Award XP based on review outcome
        self.stats.xp += match rating {
            Rating::Again => 5.0,
            _ => 1.0,
        };
    }

    fn update_daily_streak(&mut self, timestamp: &DateTime<Utc>) {
        match &self.stats.daily_streak {
            None => {
                // First review ever - streak expires 30 hours from now
                self.stats.daily_streak = Some(DailyStreak {
                    streak_start: *timestamp,
                    streak_expiry: *timestamp + chrono::Duration::hours(30),
                });
            }
            Some(streak) => {
                if timestamp < &streak.streak_expiry {
                    // Within expiry window, continue streak and extend expiry
                    self.stats.daily_streak = Some(DailyStreak {
                        streak_start: streak.streak_start,
                        streak_expiry: *timestamp + chrono::Duration::hours(30),
                    });
                } else {
                    // Past expiry, start new streak
                    self.stats.daily_streak = Some(DailyStreak {
                        streak_start: *timestamp,
                        streak_expiry: *timestamp + chrono::Duration::hours(30),
                    });
                }
                // Note: if timestamp is before streak_expiry but in the past relative to
                // streak_expiry calculation time, we still update. This handles out-of-order events.
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Deck {
    /// Helper function to create a CardSummary from a card indicator and status
    fn card_to_summary(
        &self,
        card_indicator: &CardIndicator<SpurGram, Spur>,
        card_status: &CardStatus,
    ) -> Option<CardSummary> {
        if let CardStatus::Tracked(CardData::Added { fsrs_card }) = card_status {
            let state = match fsrs_card.state {
                rs_fsrs::State::New => "new".to_string(),
                rs_fsrs::State::Learning => "learning".to_string(),
                rs_fsrs::State::Review => "review".to_string(),
                rs_fsrs::State::Relearning => "relearning".to_string(),
            };

            // Compute card_text and card_subtitle based on card type
            let (card_text, card_subtitle) = match card_indicator {
                CardIndicator::WrittenGram { gram } => {
                    let gram_resolved = self
                        .context
                        .language_pack
                        .gram_rodeo
                        .resolve(gram)
                        .resolve(&self.context.language_pack.string_rodeo);
                    let text = gram_resolved.to_display_string(self.context.course.target_language);
                    // Get POS from first heteronym if available for subtitle
                    let subtitle = gram_resolved.0.first().and_then(|atom| {
                        if let language_utils::Atom::Tok(word) = atom {
                            if let language_utils::WordType::Heteronym(h) = &word.word_type {
                                return Some(h.pos.to_string().to_lowercase());
                            }
                        }
                        None
                    });
                    (text, subtitle)
                }
                CardIndicator::ListeningGram { gram } => {
                    let gram_resolved = self
                        .context
                        .language_pack
                        .gram_rodeo
                        .resolve(gram)
                        .resolve(&self.context.language_pack.string_rodeo);
                    let text = gram_resolved.to_display_string(self.context.course.target_language);
                    (text, Some("listening".to_string()))
                }
                CardIndicator::LetterPronunciation { pattern, .. } => {
                    let text = self.context.language_pack.string_rodeo.resolve(pattern);
                    (format!("[{text}]"), Some("pronunciation".to_string()))
                }
            };

            Some(CardSummary {
                card_indicator: card_indicator.resolve(
                    &self.context.language_pack.string_rodeo,
                    &self.context.language_pack.gram_rodeo,
                ),
                due_timestamp_ms: fsrs_card.due.timestamp_millis() as f64,
                state,
                card_text,
                card_subtitle,
            })
        } else {
            None
        }
    }

    /// Returns an iterator over cards (excluding leeches)
    fn cards_excluding_leeches(
        &self,
    ) -> impl Iterator<Item = (&CardIndicator<SpurGram, Spur>, &CardStatus)> {
        self.cards
            .iter()
            .filter(|(card_indicator, _)| !self.leeches.contains_key(card_indicator))
    }

    /// Get the set of comprehensible written grams (includes both single-word and multiword grams).
    fn get_comprehensible_written_grams(&self) -> BTreeSet<SpurGram> {
        let mut comprehensible_grams = BTreeSet::new();

        for (card_indicator, card_status) in self.cards.iter() {
            if !self
                .context
                .is_comprehensible(card_indicator, card_status, &self.regressions)
            {
                continue;
            }

            if let CardIndicator::WrittenGram { gram } = card_indicator {
                comprehensible_grams.insert(*gram);
            }
        }

        comprehensible_grams
    }

    /// First, the frontend calls get_all_cards_summary to get a view of what cards are due and what cards are going to be due in the future.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_all_cards_summary(&self) -> Vec<CardSummary> {
        let mut summaries: Vec<CardSummary> = self
            .cards_excluding_leeches()
            .filter_map(|(card_indicator, card_status)| {
                self.card_to_summary(card_indicator, card_status)
            })
            .collect();

        // Sort by due date
        summaries.sort_by(|a, b| a.due_timestamp_ms.partial_cmp(&b.due_timestamp_ms).unwrap());

        summaries
    }

    /// Get all cards that have been detected as leeches (12+ lapses)
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_leeches(&self) -> Vec<CardSummary> {
        self.leeches
            .keys()
            .filter_map(|card_indicator| {
                self.cards
                    .get(card_indicator)
                    .and_then(|card_status| self.card_to_summary(card_indicator, card_status))
            })
            .collect()
    }

    /// TODO: get_review_info and get_all_cards_summary can probably be combined.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_review_info(
        &self,
        banned_challenge_types: Vec<ChallengeRequirements>,
        timestamp_ms: f64,
    ) -> ReviewInfo {
        let now =
            DateTime::<Utc>::from_timestamp_millis(timestamp_ms as i64).unwrap_or_else(Utc::now);
        let mut due_cards = vec![];
        let mut future_cards = vec![];
        let mut due_but_banned_cards = vec![];

        let no_listening_cards = banned_challenge_types.contains(&ChallengeRequirements::Listening);
        let no_text_cards = banned_challenge_types.contains(&ChallengeRequirements::Text);
        let no_speaking_cards = banned_challenge_types.contains(&ChallengeRequirements::Speaking);

        for (card, card_status) in self.cards_excluding_leeches() {
            if let CardStatus::Tracked(CardData::Added { fsrs_card }) = card_status {
                let due_date = fsrs_card.due;

                if due_date <= now {
                    match card.card_type().challenge_type() {
                        ChallengeRequirements::Text if no_text_cards => {
                            due_but_banned_cards.push(*card);
                        }
                        ChallengeRequirements::Listening if no_listening_cards => {
                            due_but_banned_cards.push(*card);
                        }
                        ChallengeRequirements::Speaking if no_speaking_cards => {
                            due_but_banned_cards.push(*card);
                        }
                        _ => due_cards.push(*card),
                    }
                } else {
                    future_cards.push(*card);
                }
            }
        }

        // sort by due date, then by card indicator for deterministic ordering
        due_cards.sort_by_key(|card_indicator| {
            let card_status = self.cards.get(card_indicator).unwrap();
            let due_timestamp = if let CardStatus::Tracked(card_data) = card_status {
                ordered_float::NotNan::new(card_data.due_timestamp_ms()).unwrap()
            } else {
                ordered_float::NotNan::new(0.0).unwrap()
            };
            (due_timestamp, *card_indicator)
        });

        due_but_banned_cards.sort_by_key(|card_indicator| {
            let card_status = self.cards.get(card_indicator).unwrap();
            let due_timestamp = if let CardStatus::Tracked(card_data) = card_status {
                ordered_float::NotNan::new(card_data.due_timestamp_ms()).unwrap()
            } else {
                ordered_float::NotNan::new(0.0).unwrap()
            };
            (due_timestamp, *card_indicator)
        });

        future_cards.sort_by_key(|card_indicator| {
            let card_status = self.cards.get(card_indicator).unwrap();
            let due_timestamp = if let CardStatus::Tracked(card_data) = card_status {
                ordered_float::NotNan::new(card_data.due_timestamp_ms()).unwrap()
            } else {
                ordered_float::NotNan::new(0.0).unwrap()
            };
            (due_timestamp, *card_indicator)
        });

        ReviewInfo {
            due_cards,
            due_but_banned_cards,
            future_cards,
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn cache_challenge_audio(
        &self,
        access_token: Option<String>,
        abort_signal: Option<web_sys::AbortSignal>,
    ) {
        let mut audio_cache = match audio::AudioCache::new().await {
            Ok(cache) => cache,
            Err(e) => {
                log::error!("Failed to create audio cache: {e:?}");
                return;
            }
        };
        let access_token = access_token.as_ref();

        const SIMULATION_DAYS: u32 = 0; // set to 0 right now in case it's causing our memory issues
        let mut requested_filenames = BTreeSet::new();
        let mut simulation_iterator = self.simulate_usage(chrono::Utc::now());
        #[expect(clippy::reversed_empty_ranges)] // Intentionally disabled (SIMULATION_DAYS = 0)
        for _ in 0..SIMULATION_DAYS {
            // Sleep for 1 second using JavaScript's setTimeout via JsFuture
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 1000)
                    .unwrap();
            });
            wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();

            // Check if aborted before progressing
            if let Some(ref signal) = abort_signal {
                if signal.aborted() {
                    return;
                }
            }

            let challenges;
            (simulation_iterator, challenges) = simulation_iterator.next();

            // get the audio files
            requested_filenames.extend(
                futures::stream::iter(challenges)
                    .map(|challenge| {
                        let request = challenge.audio_request();
                        let audio_cache = audio_cache.clone();
                        let abort_signal = abort_signal.clone();
                        async move {
                            let request = request?;
                            // Check if aborted before processing
                            if let Some(ref signal) = abort_signal {
                                if signal.aborted() {
                                    return None;
                                }
                            }

                            // Generate the cache filename for this request
                            let cache_filename = audio::AudioCache::get_cache_filename(
                                &request.request,
                                &request.provider,
                            );

                            // Just try to fetch and cache, ignoring errors for individual requests
                            let _ = audio_cache.fetch_and_cache(&request, access_token).await;
                            Some(cache_filename)
                        }
                    })
                    .buffered(3)
                    .filter_map(|x| async { x })
                    .collect::<BTreeSet<_>>()
                    .await,
            );
            // sleep for 1 second
        }

        // Check if aborted before cleanup
        if let Some(ref signal) = abort_signal {
            if signal.aborted() {
                return;
            }
        }

        // Clean up any files that weren't in the requested set
        if let Err(e) = audio_cache.cleanup_except(requested_filenames).await {
            log::error!("Failed to clean up audio cache: {e:?}");
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_percent_of_words_known(&self) -> f64 {
        let total_words_reviewed: u64 = self
            .cards_excluding_leeches()
            .filter_map(|(card_indicator, card_status)| {
                if let CardStatus::Tracked(card_data) = card_status {
                    let is_reviewed = match card_data {
                        CardData::Added { fsrs_card } => fsrs_card.state != rs_fsrs::State::New,
                        CardData::Ghost { fsrs_card } => fsrs_card.state != rs_fsrs::State::New,
                    };
                    if is_reviewed {
                        match card_indicator {
                            CardIndicator::WrittenGram { .. } => {
                                self.context.get_card_frequency(card_indicator)
                            }
                            CardIndicator::ListeningGram { .. }
                            | CardIndicator::LetterPronunciation { .. } => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .map(|freq| freq.count as u64)
            .sum();
        total_words_reviewed as f64 / self.context.language_pack.total_word_count as f64
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_total_reviews(&self) -> u64 {
        self.stats.total_reviews
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_xp(&self) -> f64 {
        self.stats.xp
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_daily_streak(&self) -> u32 {
        match &self.stats.daily_streak {
            None => 0,
            Some(streak) => {
                let now = chrono::Utc::now();

                if now < streak.streak_expiry {
                    // Streak is active (hasn't expired yet)
                    (now.date_naive() - streak.streak_start.date_naive()).num_days() as u32 + 1
                } else {
                    // Streak is broken (expired)
                    0
                }
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_movie_stats(&self) -> Vec<MovieStats> {
        let language_pack = &self.context.language_pack;
        let mut stats = Vec::new();

        let comprehensible_grams = self.get_comprehensible_written_grams();

        for movie_id in language_pack.movies.keys() {
            let Some(movie_frequencies) = language_pack.movie_gram_frequencies.get(movie_id) else {
                continue;
            };

            if movie_frequencies.is_empty() {
                continue;
            }

            // Calculate total units and comprehensible units from gram frequencies.
            let mut total_word_count = 0u64;
            let mut comprehensible_word_count = 0u64;

            for (gram, frequency) in movie_frequencies.iter() {
                let word_count = frequency.count as u64;
                total_word_count += word_count;

                if comprehensible_grams.contains(gram) {
                    comprehensible_word_count += word_count;
                }
            }

            if total_word_count == 0 {
                continue;
            }

            let percent_known =
                (comprehensible_word_count as f64 / total_word_count as f64) * 100.0;

            // Calculate cards needed to reach next 5% milestone
            let cards_to_next_milestone = if percent_known < 100.0 {
                let next_milestone = ((percent_known / 5.0).ceil() * 5.0).min(100.0);
                let target_word_count = ((next_milestone / 100.0) * total_word_count as f64) as u64;
                let words_needed = target_word_count.saturating_sub(comprehensible_word_count);

                if words_needed > 0 {
                    // Collect unknown grams with their frequencies.
                    let mut unknown_words: Vec<(SpurGram, u64)> = movie_frequencies
                        .iter()
                        .filter_map(|(gram, frequency)| {
                            if comprehensible_grams.contains(gram) {
                                None
                            } else {
                                Some((*gram, frequency.count as u64))
                            }
                        })
                        .collect();

                    // Sort by frequency descending (most common first).
                    unknown_words.sort_by(|a, b| b.1.cmp(&a.1));

                    // Count how many cards we need to learn to reach target
                    let mut accumulated_words = 0u64;
                    let mut cards_needed = 0u32;

                    for (_lexeme, count) in unknown_words {
                        if accumulated_words >= words_needed {
                            break;
                        }
                        accumulated_words += count;
                        cards_needed += 1;
                    }

                    Some(cards_needed)
                } else {
                    None
                }
            } else {
                None
            };

            stats.push(MovieStats {
                id: movie_id.clone(),
                percent_known,
                cards_to_next_milestone,
            });
        }

        // Sort by percent known descending
        stats.sort_by(|a, b| b.percent_known.partial_cmp(&a.percent_known).unwrap());

        stats
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_movie_metadata(&self, movie_ids: Vec<String>) -> Vec<MovieMetadata> {
        let language_pack = &self.context.language_pack;
        let mut movies = Vec::new();

        for movie_id in movie_ids {
            if let Some(movie_metadata) = language_pack.movies.get(&movie_id) {
                movies.push(MovieMetadata {
                    id: movie_id.clone(),
                    title: movie_metadata.title.clone(),
                    year: movie_metadata.year,
                    poster_bytes: movie_metadata.poster_bytes.clone(),
                });
            }
        }

        movies
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_target_language(&self) -> Language {
        self.context.course.target_language
    }

    fn max_cards_to_add(&self) -> usize {
        let current_cards = self.num_cards();

        if current_cards < 5 {
            1
        } else if current_cards < 11 {
            2
        } else {
            5
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn add_card_options(
        &self,
        banned_challenge_types: Vec<ChallengeRequirements>,
    ) -> AddCardOptions {
        let banned_types_set = banned_challenge_types
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();

        let max_cards_to_add = self.max_cards_to_add();

        AddCardOptions {
            manual_add: vec![
                (
                    if banned_types_set.contains(&ChallengeRequirements::Text) {
                        0
                    } else {
                        self.next_unknown_cards(AllowedCards::Type(CardType::TargetLanguage))
                            .take(max_cards_to_add)
                            .count() as u32
                    },
                    CardType::TargetLanguage,
                ),
                (
                    if banned_types_set.contains(&ChallengeRequirements::Listening) {
                        0
                    } else {
                        self.next_unknown_cards(AllowedCards::Type(CardType::Listening))
                            .take(max_cards_to_add)
                            .count() as u32
                    },
                    CardType::Listening,
                ),
                (
                    if banned_types_set.contains(&ChallengeRequirements::Speaking) {
                        0
                    } else {
                        self.next_unknown_cards(AllowedCards::Type(CardType::LetterPronunciation))
                            .take(max_cards_to_add)
                            .count() as u32
                    },
                    CardType::LetterPronunciation,
                ),
            ],
            smart_add: self
                .next_unknown_cards(AllowedCards::BannedRequirements(banned_types_set))
                .take(max_cards_to_add)
                .count() as u32,
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn add_next_unknown_cards(
        &self,
        card_type: Option<CardType>,
        count: usize,
        banned_challenge_types: Vec<ChallengeRequirements>,
    ) -> Option<DeckEvent> {
        let banned_types_set = banned_challenge_types
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();

        if count == 0 {
            return None;
        }

        let allowed_cards = match (card_type, banned_types_set) {
            (Some(card_type), _) => AllowedCards::Type(card_type),
            (None, banned_types_set) => AllowedCards::BannedRequirements(banned_types_set),
        };

        let cards = self
            .next_unknown_cards(allowed_cards)
            .take(count)
            .map(|card| {
                card.resolve(
                    &self.context.language_pack.string_rodeo,
                    &self.context.language_pack.gram_rodeo,
                )
            })
            .collect::<Vec<_>>();

        (!cards.is_empty()).then_some({
            DeckEvent::Language(LanguageEvent {
                target_language: self.context.course.target_language,
                native_language: self.context.course.native_language,
                content: LanguageEventContent::AddCards { cards },
            })
        })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn complete_placement_test(
        &self,
        known_words: Vec<String>,
        unknown_words: Vec<String>,
    ) -> DeckEvent {
        DeckEvent::Language(LanguageEvent {
            target_language: self.context.course.target_language,
            native_language: self.context.course.native_language,
            content: LanguageEventContent::CompletePlacementTest {
                results: PlacementTest {
                    known_words,
                    unknown_words,
                },
            },
        })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn review_card(
        &self,
        reviewed: CardIndicator<Gram<String>, String>,
        rating: Rating,
    ) -> Option<DeckEvent> {
        let indicator = reviewed.get_interned(
            &self.context.language_pack.string_rodeo,
            &self.context.language_pack.gram_rodeo,
        )?;
        self.cards.get(&indicator).and_then(|status| {
            matches!(status, CardStatus::Tracked(_)).then_some(DeckEvent::Language(LanguageEvent {
                target_language: self.context.course.target_language,
                native_language: self.context.course.native_language,
                content: LanguageEventContent::ReviewCard { reviewed, rating },
            }))
        })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn translate_sentence_perfect(
        &self,
        words_tapped: Vec<Heteronym<String>>,
        challenge_sentence: String,
    ) -> Option<DeckEvent> {
        let hinted_heteronyms: BTreeSet<Heteronym<String>> = words_tapped.into_iter().collect();

        let cleaned_sentence = language_utils::text_cleanup::cleanup_sentence(
            challenge_sentence.clone(),
            self.context.course.target_language,
        );
        let sentence_spur = self
            .context
            .language_pack
            .string_rodeo
            .get(&cleaned_sentence)?;
        let sentence_literals = self
            .context
            .language_pack
            .sentence_to_literals(&sentence_spur, self.context.course.target_language)?;

        let literals = sentence_literals
            .into_iter()
            .map(|literal| {
                let hinted = match &literal.word.word_type {
                    WordType::Heteronym(h) => Some(hinted_heteronyms.contains(h)),
                    WordType::Other(_) => None,
                };
                (literal, hinted)
            })
            .collect();

        let review = SentenceReviewResult::Perfect {
            challenge: challenge_sentence.clone(),
            submission: challenge_sentence,
            literals,
        };

        Some(DeckEvent::Language(LanguageEvent {
            target_language: self.context.course.target_language,
            native_language: self.context.course.native_language,
            content: LanguageEventContent::TranslationChallenge {
                review,
                legacy: LegacyTranslationChallenge::default(),
            },
        }))
    }

    /// Create a Graded translation challenge event.
    ///
    /// `literal_grades` should have one entry per literal in the sentence (same order as
    /// `target_language_literals` from the challenge). None for Other word types or unknown,
    /// Some(Remembered/Forgot) for heteronyms.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn translate_sentence_wrong(
        &self,
        challenge_sentence: String,
        submission: String,
        literal_grades: autograde::LiteralGrades,
        words_tapped: Vec<Heteronym<String>>,
        phrases_remembered: Vec<String>,
        phrases_forgot: Vec<String>,
    ) -> Option<DeckEvent> {
        let literal_grades = literal_grades.0;
        let hinted_heteronyms: BTreeSet<Heteronym<String>> = words_tapped.into_iter().collect();

        let cleaned_sentence = language_utils::text_cleanup::cleanup_sentence(
            challenge_sentence.clone(),
            self.context.course.target_language,
        );
        let sentence_spur = self
            .context
            .language_pack
            .string_rodeo
            .get(&cleaned_sentence)?;
        let sentence_literals = self
            .context
            .language_pack
            .sentence_to_literals(&sentence_spur, self.context.course.target_language)?;

        // Zip literal_grades with sentence_literals to build the event
        let literals: Vec<_> = sentence_literals
            .into_iter()
            .zip(literal_grades.iter())
            .map(|(literal, grade)| {
                let result = match (&literal.word.word_type, grade) {
                    (WordType::Heteronym(h), Some(remembered)) => Some(current::LiteralResult {
                        remembered: Some(*remembered == autograde::Remembered::Remembered),
                        hinted: hinted_heteronyms.contains(h),
                    }),
                    (WordType::Heteronym(h), None) => {
                        // Grade is unknown/indeterminate
                        Some(current::LiteralResult {
                            remembered: None,
                            hinted: hinted_heteronyms.contains(h),
                        })
                    }
                    (WordType::Other(_), _) => None,
                };
                (literal, result)
            })
            .collect();

        // Build phrases list - forgot takes precedence over remembered
        let forgot_set: BTreeSet<&String> = phrases_forgot.iter().collect();
        let phrases: Vec<_> = phrases_forgot
            .iter()
            .map(|p| (p.clone(), Some(false)))
            .chain(
                phrases_remembered
                    .iter()
                    .filter(|p| !forgot_set.contains(p))
                    .map(|p| (p.clone(), Some(true))),
            )
            .collect();

        let review = SentenceReviewResult::Graded {
            challenge: challenge_sentence,
            submission,
            literals,
            phrases,
        };

        Some(DeckEvent::Language(LanguageEvent {
            target_language: self.context.course.target_language,
            native_language: self.context.course.native_language,
            content: LanguageEventContent::TranslationChallenge {
                review,
                legacy: LegacyTranslationChallenge::default(),
            },
        }))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn transcribe_sentence(
        &self,
        challenge: Vec<transcription_challenge::PartGraded>,
    ) -> Option<DeckEvent> {
        Some(DeckEvent::Language(LanguageEvent {
            target_language: self.context.course.target_language,
            native_language: self.context.course.native_language,
            content: LanguageEventContent::TranscriptionChallenge { challenge },
        }))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn num_cards(&self) -> usize {
        self.cards.values().filter_map(CardStatus::reviewed).count()
    }

    /// Get the average number of challenges completed per day in the past week
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_past_week_challenge_average(&self) -> f64 {
        let total_challenges: u32 = self.stats.past_week_challenges.values().sum();
        // Average over 7 days
        total_challenges as f64 / 7.0
    }

    /// Calculate upcoming review statistics for the next three weeks
    /// Returns total reviews and max reviews on any single day
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_upcoming_week_review_stats(&self) -> UpcomingReviewStats {
        let now = Utc::now();
        let three_weeks_later = now + chrono::Duration::days(21);

        let mut daily_counts: FxHashMap<i64, u32> = FxHashMap::default();
        let mut total_reviews = 0u32;

        for (_, card_status) in self.cards.iter() {
            if let CardStatus::Tracked(CardData::Added { fsrs_card }) = card_status {
                let due_date = fsrs_card.due;

                // Skip new cards (they haven't been reviewed yet)
                if fsrs_card.state == rs_fsrs::State::New {
                    continue;
                }

                // Check if due within the next three weeks
                if due_date > now && due_date <= three_weeks_later {
                    total_reviews += 1;

                    // Get the day offset from today (0 = today, 1 = tomorrow, etc.)
                    let days_from_now = (due_date - now).num_days();
                    *daily_counts.entry(days_from_now).or_insert(0) += 1;
                }
            }
        }

        let max_per_day = daily_counts.values().max().copied().unwrap_or(0);

        UpcomingReviewStats {
            total_reviews,
            max_per_day,
        }
    }

    /// Count the number of cards created within the past `hours` hours.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_cards_added_in_past_hours(&self, hours: f64) -> u32 {
        if !hours.is_finite() || hours <= 0.0 {
            return 0;
        }

        let clamped_hours = hours.min((i64::MAX as f64) / 3600.0);
        let cutoff =
            Utc::now() - chrono::Duration::seconds((clamped_hours * 3600.0).round() as i64);

        self.cards
            .values()
            .filter_map(|card_status| match card_status {
                CardStatus::Tracked(CardData::Added { fsrs_card }) => Some(fsrs_card),
                _ => None,
            })
            .filter(|fsrs_card| fsrs_card.created_at >= cutoff)
            .count() as u32
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_frequency_knowledge_chart_data(&self) -> Vec<FrequencyKnowledgePoint> {
        // Sample frequencies from 1 to 10000 on a logarithmic scale
        let target_frequencies: Vec<f64> = vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 15.0, 20.0, 30.0, 40.0, 50.0, 60.0,
            70.0, 80.0, 90.0, 100.0, 150.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0,
            1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0,
            10000.0,
        ];

        // Create a map to collect data for each frequency bucket
        let mut frequency_buckets: FxHashMap<String, (Vec<f64>, Vec<String>)> =
            FxHashMap::default();

        // Iterate through actual grams/phrases in the language pack and find ones matching our target frequencies
        for (gram, frequency) in self.context.language_pack.gram_frequencies.iter() {
            let freq_value = frequency.count as f64;

            // Check if this frequency is close to one of our target frequencies
            for &target_freq in &target_frequencies {
                if (freq_value - target_freq).abs() < target_freq * 0.1 {
                    // Within 10% of target
                    let card_indicator = CardIndicator::WrittenGram { gram: *gram };

                    // Use the regression to predict knowledge at this frequency
                    let knowledge_probability = self
                        .regressions
                        .predict_card_knowledge_probability(&card_indicator, *frequency);

                    // Get display text for the card
                    let display_text = self
                        .context
                        .language_pack
                        .gram_rodeo
                        .resolve(gram)
                        .resolve(&self.context.language_pack.string_rodeo)
                        .to_display_string(self.context.course.target_language);

                    let bucket_key = format!("{target_freq}");
                    let entry = frequency_buckets
                        .entry(bucket_key)
                        .or_insert((vec![], vec![]));
                    entry.0.push(knowledge_probability);
                    if entry.1.len() < 5 {
                        // Limit to 5 example words per bucket
                        entry.1.push(display_text);
                    }

                    break;
                }
            }
        }

        // Convert buckets to final chart data
        let mut chart_data = Vec::new();
        for &target_freq in &target_frequencies {
            let bucket_key = format!("{target_freq}");
            if let Some((probabilities, words)) = frequency_buckets.get(&bucket_key) {
                if !probabilities.is_empty() {
                    let avg_probability =
                        probabilities.iter().sum::<f64>() / probabilities.len() as f64;
                    chart_data.push(FrequencyKnowledgePoint {
                        frequency: target_freq,
                        predicted_knowledge: avg_probability,
                        word_count: probabilities.len() as u32,
                        example_words: words.join(", "),
                    });
                }
            }
        }

        chart_data
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn has_taken_placement_test(&self) -> bool {
        self.placement_test_results.is_some()
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct UpcomingReviewStats {
    pub total_reviews: u32,
    pub max_per_day: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
pub struct FrequencyKnowledgePoint {
    pub frequency: f64,
    pub predicted_knowledge: f64,
    pub word_count: u32,
    pub example_words: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
pub struct MovieStats {
    pub id: String,
    pub percent_known: f64,
    pub cards_to_next_milestone: Option<u32>,
}

impl Deck {
    pub(crate) fn next_unknown_cards(&self, allowed_cards: AllowedCards) -> NextCardsIterator<'_> {
        NextCardsIterator::new(self, allowed_cards)
    }

    fn get_comprehensible_sentence_containing(
        &self,
        required_gram: Option<&SpurGram>,
        mut comprehensible_grams: BTreeSet<SpurGram>,
        sentences_reviewed: &BTreeMap<Spur, u32>,
        language_pack: &LanguagePack,
    ) -> Option<ComprehensibleSentence> {
        // Add the target gram to comprehensible set if provided
        if let Some(required) = required_gram {
            comprehensible_grams.insert(*required);
        }

        // Search through all sentences - if we have a required gram, only look at sentences containing it
        let candidate_sentences: Vec<Spur> = if let Some(required) = required_gram {
            language_pack
                .sentences_containing_gram_index
                .get(required)?
                .clone()
        } else {
            // If no required gram/phrase, consider all sentences
            language_pack.translations.keys().cloned().collect()
        };

        let mut possible_sentences = Vec::new();

        // Warning: this loop is HOT!
        'checkSentences: for sentence in &candidate_sentences {
            let Some(sentence_grams) = language_pack.encoded_sentences.get(sentence) else {
                continue;
            };

            // Check that all learnable grams are comprehensible
            for sentence_gram in &sentence_grams.grams {
                if let SentenceGram::Learnable(gram) = sentence_gram {
                    if !comprehensible_grams.contains(gram) {
                        continue 'checkSentences; // Early exit!
                    }
                }
            }

            // Check that all high-confidence multiword terms are comprehensible
            for multiword_gram in &sentence_grams.multiword_terms {
                if !comprehensible_grams.contains(multiword_gram) {
                    continue 'checkSentences; // Early exit!
                }
            }

            possible_sentences.push(sentence);
        }

        if !possible_sentences.is_empty() {
            possible_sentences.sort_by_key(|sentence| {
                let sentence_review_count = sentences_reviewed.get(sentence).unwrap_or(&0);
                *sentence_review_count
            });
            let target_language_sentence = **possible_sentences.first()?;

            let sentence_grams = language_pack
                .encoded_sentences
                .get(&target_language_sentence)?;

            // Collect unique phrases (high-confidence multiword terms)
            let unique_target_language_phrases = {
                let mut unique_phrases = vec![];
                let mut phrases_set = BTreeSet::new();

                for phrase in &sentence_grams.multiword_terms {
                    if !phrases_set.contains(phrase) {
                        unique_phrases.push(*phrase);
                        phrases_set.insert(*phrase);
                    }
                }
                unique_phrases
            };

            let native_languages = language_pack
                .translations
                .get(&target_language_sentence)
                .unwrap()
                .clone();

            return Some(ComprehensibleSentence {
                target_language: target_language_sentence,
                target_language_sentence_grams: sentence_grams.clone(),
                unique_target_language_phrases,
                native_languages,
            });
        }

        None
    }

    fn is_listened_gram_comprehensible(&self, gram: &SpurGram) -> bool {
        let card_indicator = CardIndicator::ListeningGram { gram: *gram };
        let Some(fsrs_card) = self.cards.get(&card_indicator) else {
            return false;
        };
        self.context
            .is_comprehensible(&card_indicator, fsrs_card, &self.regressions)
    }
}

impl Context {
    /// Check if a card is valid and can be added to the deck
    /// For lexeme cards: checks if they exist in word_frequencies (which guarantees they have definitions)
    /// For listening cards: checks if the pronunciation exists
    /// For letter pronunciation cards: checks if the pattern exists in the frequency map
    pub fn is_card_valid(&self, card: &CardIndicator<SpurGram, Spur>) -> bool {
        match card {
            CardIndicator::WrittenGram { gram } | CardIndicator::ListeningGram { gram } => {
                self.language_pack.gram_frequencies.contains_key(gram)
            }
            CardIndicator::LetterPronunciation { pattern, position } => self
                .language_pack
                .pattern_frequency_map
                .contains_key(&(*pattern, *position)),
        }
    }

    fn is_comprehensible(
        &self,
        card_indicator: &CardIndicator<SpurGram, Spur>,
        card_status: &CardStatus,
        regressions: &Regressions,
    ) -> bool {
        match card_status {
            // For tracked cards (both Added and Ghost), check if they're in review state
            CardStatus::Tracked(card_data) => {
                match card_data {
                    CardData::Added { fsrs_card } | CardData::Ghost { fsrs_card } => {
                        // Card is comprehensible if it's in review state (not new, learning, or relearning)
                        fsrs_card.state == rs_fsrs::State::Review
                    }
                }
            }
            // For unadded cards, use regression predictions
            CardStatus::Unadded(_) => {
                // Check if we have high confidence they would be known
                // Use 80% probability threshold for considering a card comprehensible
                // 80% was not chosen in a super scientific way, it's just a number that seemed to work well
                if let Some((knowledge_probability, _)) =
                    self.get_card_knowledge_probability(card_indicator, regressions)
                {
                    knowledge_probability >= 0.80
                } else {
                    false
                }
            }
        }
    }

    fn get_card_value(
        &self,
        card: &CardIndicator<SpurGram, Spur>,
        regressions: &Regressions,
    ) -> Option<ordered_float::NotNan<f64>> {
        let (knowledge_probability, frequency) =
            self.get_card_knowledge_probability(card, regressions)?;
        ordered_float::NotNan::new((1.0 - knowledge_probability) * (frequency.ln_frequency())).ok()
    }

    fn get_card_value_with_status(
        &self,
        card: &CardIndicator<SpurGram, Spur>,
        status: &CardStatus,
        regressions: &Regressions,
    ) -> Option<ordered_float::NotNan<f64>> {
        let frequency = self.get_card_frequency(card)?;

        // Check if we have a reviewed card (ghost or added)
        if let CardStatus::Tracked(card_data) = status {
            // Get the FSRS card using explicit pattern match
            let fsrs_card = match card_data {
                CardData::Added { fsrs_card } | CardData::Ghost { fsrs_card } => fsrs_card,
            };

            // If it's been reviewed (not new), use the actual knowledge from FSRS
            if fsrs_card.state != rs_fsrs::State::New {
                // Get the predicted knowledge
                let predicted_knowledge = regressions.predict_card_knowledge(card, frequency)?;

                // Calculate observed knowledge from FSRS data
                let observed_knowledge = if fsrs_card.lapses == 0 {
                    fsrs_card.accumulated_positive_surprise
                } else {
                    -fsrs_card.accumulated_negative_surprise
                };

                // For ghost cards, combine observed and predicted
                // For added cards, just use observed
                let combined_knowledge = match card_data {
                    CardData::Ghost { .. } => {
                        if observed_knowledge < 0.0 {
                            // Has lapses: use whichever is lower (more pessimistic)
                            observed_knowledge.min(predicted_knowledge)
                        } else {
                            // No lapses: add positive surprisal to prediction
                            observed_knowledge + predicted_knowledge
                        }
                    }
                    CardData::Added { .. } => {
                        // Added card - use actual knowledge
                        observed_knowledge
                    }
                };

                // Convert knowledge to probability and then to value
                let probability = Regressions::knowledge_to_probability(combined_knowledge);
                return ordered_float::NotNan::new((1.0 - probability) * frequency.ln_frequency())
                    .ok();
            }
        }

        // Fall back to regular prediction-based value for new or unadded cards
        self.get_card_value(card, regressions)
    }

    fn get_card_knowledge_probability(
        &self,
        card: &CardIndicator<SpurGram, Spur>,
        regressions: &Regressions,
    ) -> Option<(f64, Frequency)> {
        let frequency = self.get_card_frequency(card)?;

        let knowledge_probability = match card {
            CardIndicator::LetterPronunciation { pattern, position } => {
                // For pronunciation patterns, use the LLM's familiarity assessment
                let pattern_str = self.language_pack.string_rodeo.resolve(pattern);
                let guide = self
                    .language_pack
                    .pronunciation_data
                    .guides
                    .iter()
                    .find(|g| g.pattern == pattern_str && g.position == *position)?;

                // Convert familiarity to probability
                match guide.familiarity {
                    language_utils::PronunciationFamiliarity::LikelyAlreadyKnows => 0.85,
                    language_utils::PronunciationFamiliarity::MaybeAlreadyKnows => 0.50,
                    language_utils::PronunciationFamiliarity::ProbablyDoesNotKnow => 0.15,
                }
            }
            _ => regressions.predict_card_knowledge_probability(card, frequency),
        };

        Some((knowledge_probability, frequency))
    }

    /// Get the frequency count for a card (used for isotonic regression)
    fn get_card_frequency(&self, card: &CardIndicator<SpurGram, Spur>) -> Option<Frequency> {
        match card {
            CardIndicator::WrittenGram { gram } | CardIndicator::ListeningGram { gram } => {
                self.language_pack.gram_frequencies.get(gram).copied()
            }
            CardIndicator::LetterPronunciation { pattern, position } => {
                // Look up the actual frequency of this pattern from our calculated data
                let count = self
                    .language_pack
                    .pattern_frequency_map
                    .get(&(*pattern, *position))
                    .copied()
                    .unwrap_or(0);
                Some(Frequency { count })
            }
        }
    }

    #[allow(unused)] // for the future "know the difference" cards
    fn get_homophone_practice(&self, word1: Spur, word2: Spur) -> Option<&HomophonePractice<Spur>> {
        self.language_pack
            .homophone_practice
            .get(&HomophoneWordPair { word1, word2 })
            .or_else(|| {
                self.language_pack
                    .homophone_practice
                    .get(&HomophoneWordPair {
                        word1: word2,
                        word2: word1,
                    })
            })
    }

    /// Look up a word string and return the most common heteronym with its frequency.
    pub(crate) fn lookup_word(&self, word_str: &str) -> Option<(Heteronym<Spur>, Frequency)> {
        let rodeo = &self.language_pack.string_rodeo;
        let words_to_heteronyms = &self.language_pack.words_to_heteronyms;

        let word_spur = rodeo.get(word_str)?;

        // Try heteronyms - find the first one that has a gram in gram_frequencies
        if let Some(heteronyms) = words_to_heteronyms.get(&word_spur) {
            for heteronym in heteronyms {
                if let Some(grams) = self.language_pack.heteronym_to_grams.get(heteronym) {
                    if let Some(gram) = grams.first() {
                        if let Some(freq) = self.language_pack.gram_frequencies.get(gram) {
                            return Some((*heteronym, *freq));
                        }
                    }
                }
            }
        }

        None
    }

    pub(crate) fn is_word_easy(&self, word: &Heteronym<Spur>) -> bool {
        // todo: probably move this to frequency entry?
        if word.pos == PartOfSpeech::Intj {
            return false;
        }
        let Some(grams) = self.language_pack.heteronym_to_grams.get(word) else {
            return false;
        };
        let Some(gram_def) = grams
            .iter()
            .find_map(|g| self.language_pack.gram_definitions.get(g))
        else {
            return false;
        };
        let GramDefinition::Dictionary(entry) = gram_def else {
            return false;
        };
        if entry.definitions.len() > 1 {
            return false;
        }
        let Some(definition) = entry.definitions.first() else {
            return false;
        };
        if definition.native.contains(" ") {
            return false;
        }
        if definition.native == entry.target_language_word {
            return false;
        }
        definition.cognate && !definition.false_cognate
    }
}

impl Regressions {
    /// Predict the pre-existing knowledge of a card based on its frequency using isotonic regression
    /// Returns None if the card type has no regression model or frequency can't be determined
    pub(crate) fn predict_card_knowledge(
        &self,
        card: &CardIndicator<SpurGram, Spur>,
        frequency: Frequency,
    ) -> Option<f64> {
        let regression = match card {
            CardIndicator::WrittenGram { .. } => self.target_language_regression.as_ref(),
            CardIndicator::ListeningGram { .. } => self.listening_regression.as_ref(),
            CardIndicator::LetterPronunciation { .. } => {
                // For pronunciation patterns, we don't use regression
                // Instead we use the LLM's familiarity assessment in predict_card_knowledge_probability
                return None;
            }
        }?;

        regression.interpolate(frequency.ln_frequency())
    }

    /// Get the predicted probability of knowing a card (0.0 to 1.0).
    /// Based on accumulated surprise (pre-existing knowledge) from review history.
    /// The relationship maps knowledge to probability:
    ///
    /// - Knowledge >= 3.0 = 95% chance of knowing (easy cards)
    /// - Knowledge = 0 = 50% chance of knowing (neutral)
    /// - Knowledge <= -2.0 = 10% chance of knowing (failed cards)
    /// - Linear interpolation between these points
    pub(crate) fn predict_card_knowledge_probability(
        &self,
        card: &CardIndicator<SpurGram, Spur>,
        frequency: Frequency,
    ) -> f64 {
        let Some(knowledge) = self.predict_card_knowledge(card, frequency) else {
            return 0.0;
        };
        Self::knowledge_to_probability(knowledge)
    }

    fn knowledge_to_probability(knowledge: f64) -> f64 {
        // With pre-existing knowledge:
        // - Positive values indicate easier cards (higher probability)
        // - Negative values indicate harder cards (lower probability)
        // - Any negative value indicates at least one lapse
        //
        // Based on latest test results:
        //   - Easy review gives ~4.6 positive surprise
        //   - Good review gives ~2.3 positive surprise initially
        //   - Initial again review gives ~0.1 negative surprise
        //   - Again after success gives ~2.4 negative surprise

        // Key insight: negative values (lapses > 0) always indicate struggling cards
        if knowledge < 0.0 {
            // Card has been failed at least once
            // New algorithm: initial failures have small negative (~0.1)
            // Failures after success have larger negative (~2.4)

            if knowledge >= -0.15 {
                // Very small negative (likely initial failure ~0.1): 10-15% probability
                // Initial failures indicate genuine lack of knowledge
                0.10 + 0.05 * ((knowledge + 0.15) / 0.15)
            } else if knowledge >= -1.0 {
                // Small to moderate negative: 5-10% probability
                let range = 1.0 - 0.15;
                0.05 + 0.05 * ((knowledge + 1.0) / range)
            } else if knowledge >= -3.0 {
                // Significant negative (failed after knowing ~2.4): 2-5% probability
                let range = 3.0 - 1.0;
                0.02 + 0.03 * ((knowledge + 3.0) / range)
            } else {
                // Deep negative surprise: cap at 2%
                0.02
            }
        } else {
            // Card has never been failed (positive knowledge)
            // Map positive surprise to higher probability
            const EASY_THRESHOLD: f64 = 4.4; // Easy review level (~4.6)
            const GOOD_THRESHOLD: f64 = 2.0; // Good review level (~2.3)

            if knowledge >= EASY_THRESHOLD {
                // Easy-level knowledge: 90-95% probability
                0.99
            } else if knowledge >= GOOD_THRESHOLD {
                // Good-level knowledge: 70-99% probability
                let range = EASY_THRESHOLD - GOOD_THRESHOLD;
                0.7 + 0.29 * (knowledge - GOOD_THRESHOLD) / range
            } else if knowledge > 0.0 {
                // Low positive knowledge: 10-70% probability
                let range = GOOD_THRESHOLD;
                0.1 + 0.6 * knowledge / range
            } else {
                // Zero knowledge (new card): 10% probability
                0.1
            }
        }
    }
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub enum CardContent {
    Gram {
        gram: Vec<Literal<String>>,
        definition: GramDefinition,
    },
    Listening {
        possible_grams: Vec<(bool, Vec<Literal<String>>)>,
    },
    LetterPronunciation {
        pattern: String,
        guide: PronunciationGuide,
    },
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug, Clone)]
pub struct ReviewInfo {
    due_cards: Vec<CardIndicator<SpurGram, Spur>>,
    due_but_banned_cards: Vec<CardIndicator<SpurGram, Spur>>,
    future_cards: Vec<CardIndicator<SpurGram, Spur>>,
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct FlashCard {
    pub content: CardContent,
    pub audio: Option<AudioRequest>,
    pub listening_prefix: Option<String>,
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub enum Challenge<G> {
    FlashCardReview {
        indicator: CardIndicator<G, String>,
        flashcard: FlashCard,
        is_new: bool,
        times_type_seen: u32,
    },
    TranslateComprehensibleSentence(TranslateComprehensibleSentence),
    TranscribeComprehensibleSentence(TranscribeComprehensibleSentence),
}

impl<G> Challenge<G> {
    fn audio_request(&self) -> Option<AudioRequest> {
        match self {
            Challenge::FlashCardReview { flashcard, .. } => flashcard.audio.clone(),
            Challenge::TranslateComprehensibleSentence(translate_comprehensible_sentence) => {
                Some(translate_comprehensible_sentence.audio.clone())
            }
            Challenge::TranscribeComprehensibleSentence(transcribe_comprehensible_sentence) => {
                Some(transcribe_comprehensible_sentence.audio.clone())
            }
        }
    }
}

#[derive(
    tsify::Tsify,
    Eq,
    PartialEq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
    Copy,
    PartialOrd,
    Ord,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum ChallengeRequirements {
    Text,
    Listening,
    Speaking,
}

impl ReviewInfo {
    // TODO: make this more resillient by separating it into a function that fallibly a real challenge and a function that tries to call the previous and returns a flashcard if it fails
    pub fn get_challenge_for_card(
        &self,
        deck: &Deck,
        card_indicator: CardIndicator<SpurGram, Spur>,
    ) -> Option<Challenge<Gram<String>>> {
        let ctx = challenge::CardContext::new(deck, card_indicator)?;

        let challenge = match card_indicator {
            CardIndicator::ListeningGram { gram } => {
                self.listening_gram_challenge(deck, &ctx, gram)
            }
            CardIndicator::WrittenGram { gram } => self.written_challenge(deck, &ctx, gram),
            CardIndicator::LetterPronunciation { pattern, position } => {
                let flashcard = self.pronunciation_pattern_flashcard(deck, pattern, position);
                ctx.wrap_flashcard(deck, flashcard)
            }
        };

        Some(challenge)
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl ReviewInfo {
    fn get_listening_prefix(language: Language) -> &'static str {
        match language {
            Language::French => "Le mot est",
            Language::Spanish => "La palabra es",
            Language::English => "The word is",
            Language::Korean => "단어는",
            Language::German => "Das Wort ist",
            Language::Chinese => "单词是",
            Language::Japanese => "単語は",
            Language::Russian => "слово",
            Language::Portuguese => "A palavra é",
            Language::Italian => "La parola è",
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_next_challenge(&self, deck: &Deck) -> Option<Challenge<Gram<String>>> {
        if let Some(due_card) = self.due_cards.first() {
            Some(self.get_challenge_for_card(deck, *due_card)?)
        } else {
            None
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl ReviewInfo {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn due_count(&self) -> usize {
        self.due_cards.len()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn due_but_banned_count(&self) -> usize {
        self.due_but_banned_cards.len()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn future_count(&self) -> usize {
        self.future_cards.len()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn total_count(&self) -> usize {
        self.due_cards.len() + self.future_cards.len()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct CardSummary {
    card_indicator: CardIndicator<Gram<String>, String>,
    due_timestamp_ms: f64,
    state: String,
    /// Primary display text for the card (e.g., the word or phrase)
    card_text: String,
    /// Optional subtitle for disambiguation (e.g., POS tag when multiple cards have same text)
    card_subtitle: Option<String>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl CardSummary {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn card_indicator(&self) -> CardIndicator<Gram<String>, String> {
        self.card_indicator.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn due_timestamp_ms(&self) -> f64 {
        self.due_timestamp_ms
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn state(&self) -> String {
        self.state.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn card_text(&self) -> String {
        self.card_text.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn card_subtitle(&self) -> Option<String> {
        self.card_subtitle.clone()
    }
}

#[wasm_bindgen]
pub fn test_fn(f: js_sys::Function) {
    f.call0(&JsValue::NULL).unwrap();
}

/// Generates a grammatical prefix for a word based on its morphology and part of speech.
/// Returns the prefix and separator, or null if no prefix is appropriate.
#[wasm_bindgen]
pub fn get_word_prefix(
    morphology: &Morphology,
    word: &str,
    pos: PartOfSpeech,
    language: Language,
) -> Option<WordPrefix> {
    morphology.get_prefix(word, pos, language)
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AudioRequest {
    request: TtsRequest,
    provider: TtsProvider,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn get_audio(
    request: AudioRequest,
    access_token: Option<String>,
) -> Result<js_sys::Uint8Array, JsValue> {
    let audio_cache = audio::AudioCache::new().await?;
    let bytes = audio_cache
        .fetch_and_cache(&request, access_token.as_ref())
        .await?;
    Ok(js_sys::Uint8Array::from(&bytes[..]))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn invalidate_audio_cache(request: AudioRequest) -> Result<(), JsValue> {
    let audio_cache = audio::AudioCache::new().await?;
    audio_cache
        .remove_cached(&request.request, &request.provider)
        .await
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn find_closest_translation(
    user_translation: String,
    candidates: Vec<String>,
    language: Language,
) -> Option<String> {
    find_closest_match(&user_translation, &candidates, language)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn autograde_translation(
    challenge_sentence: String,
    user_sentence: String,
    native_translations: Vec<String>,
    literals: Vec<Literal<String>>,
    phrases: Vec<String>,
    access_token: Option<String>,
    course: Course,
) -> Result<autograde::AutoGradeTranslationResponse, JsValue> {
    // Check if the user's translation matches any of the acceptable translations
    let normalized_user = normalize_for_grading(&user_sentence, course.native_language);
    let is_perfect = native_translations.iter().any(|translation| {
        normalize_for_grading(translation, course.native_language) == normalized_user
    });

    if is_perfect {
        // Skip server call and return perfect response
        // One entry per literal: Some(Remembered) for heteronyms, None for Other types
        let literal_grades = literals
            .iter()
            .map(|lit| {
                if lit.word.heteronym().is_some() {
                    Some(autograde::Remembered::Remembered)
                } else {
                    None
                }
            })
            .collect();

        return Ok(autograde::AutoGradeTranslationResponse {
            literal_grades,
            phrases_remembered: phrases,
            phrases_forgot: vec![],
            encouragement: Some("Perfect! You translated it correctly!".to_string()),
            explanation: None,
        });
    }

    let request = autograde::AutoGradeTranslationRequest {
        challenge_sentence,
        user_sentence,
        literals,
        phrases,
        course,
    };

    let response = hit_ai_server(
        fetch_happen::Method::POST,
        "/autograde-translation",
        Some(request),
        access_token.as_ref(),
    )
    .await
    .map_err(|e| JsValue::from_str(&format!("Request error: {e:?}")))?;

    if !response.ok() {
        return Err(JsValue::from_str(&format!(
            "HTTP error: {}",
            response.status()
        )));
    }

    let response: autograde::AutoGradeTranslationResponse = response
        .json()
        .await
        .map_err(|e| JsValue::from_str(&format!("Response parsing error: {e:?}")))?;

    log::info!("Autograde response: {response:#?}");

    Ok(response)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn autograde_transcription(
    submission: Vec<transcription_challenge::PartSubmitted>,
    access_token: Option<String>,
    course: Course,
) -> transcription_challenge::Grade {
    let _autograde_error =
        match autograde_transcription_llm(submission.clone(), access_token, course).await {
            Ok(grade) => return grade,
            Err(e) => Some(e),
        };

    // fall back to some heuristic grading
    let results = submission
        .into_iter()
        .map(|part| match part {
            transcription_challenge::PartSubmitted::AskedToTranscribe { parts, submission } => {
                let submitted_words = submission.split_whitespace().collect::<Vec<_>>();
                if submitted_words.len() != parts.len() {
                    return transcription_challenge::PartGraded::AskedToTranscribe {
                        parts: parts
                            .iter()
                            .map(|part| transcription_challenge::PartGradedPart {
                                heard: part.clone(),
                                grade: transcription_challenge::WordGrade::Missed {},
                            })
                            .collect(),
                        submission: submission.clone(),
                    };
                }

                transcription_challenge::PartGraded::AskedToTranscribe {
                    parts: parts
                        .iter()
                        .zip(submitted_words.iter())
                        .map(|(part, &submission)| {
                            let part_text =
                                normalize_for_grading(&part.word.text, course.target_language)
                                    .trim()
                                    .to_string();
                            let submission =
                                normalize_for_grading(submission, course.target_language)
                                    .trim()
                                    .to_string();
                            if part_text == submission {
                                transcription_challenge::PartGradedPart {
                                    heard: part.clone(),
                                    grade: transcription_challenge::WordGrade::Perfect {
                                        wrote: Some(submission.to_string()),
                                    },
                                }
                            } else if remove_accents(&part_text) == remove_accents(&submission) {
                                transcription_challenge::PartGradedPart {
                                    heard: part.clone(),
                                    grade: transcription_challenge::WordGrade::CorrectWithTypo {
                                        wrote: Some(submission.to_string()),
                                    },
                                }
                            // todo: check if word entered is in the set of homophones
                            // and if so, grade is as correct PhoneticallyIdenticalButContextuallyIncorrect
                            } else {
                                transcription_challenge::PartGradedPart {
                                    heard: part.clone(),
                                    grade: transcription_challenge::WordGrade::Incorrect {
                                        wrote: Some(submission.to_string()),
                                    },
                                }
                            }
                        })
                        .collect(),
                    submission: submission.clone(),
                }
            }
            transcription_challenge::PartSubmitted::Provided { part } => {
                transcription_challenge::PartGraded::Provided { part }
            }
        })
        .collect();

    transcription_challenge::Grade {
        encouragement: None,
        explanation: None,
        results,
        compare: Vec::new(),
        autograding_error: Some("The LLM was not able to grade this transcription".to_string()),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn autograde_transcription_llm(
    submission: Vec<transcription_challenge::PartSubmitted>,
    access_token: Option<String>,
    course: Course,
) -> Result<transcription_challenge::Grade, JsValue> {
    // Check if all answers are exactly correct (case-insensitive)
    let all_correct = submission.iter().all(|part| match part {
        transcription_challenge::PartSubmitted::AskedToTranscribe { parts, submission } => {
            let submission = normalize_for_grading(submission.trim(), course.target_language);
            let parts = parts
                .iter()
                .map(|part| {
                    format!(
                        "{text}{whitespace}",
                        text = normalize_for_grading(&part.word.text, course.target_language),
                        whitespace = part.whitespace
                    )
                })
                .collect::<Vec<_>>();
            submission.trim() == parts.join("").trim()
        }
        transcription_challenge::PartSubmitted::Provided { .. } => true,
    });
    if all_correct {
        // Skip server call and return perfect results
        let results = submission
            .into_iter()
            .map(|part| match part {
                transcription_challenge::PartSubmitted::AskedToTranscribe { parts, submission } => {
                    let parts = parts
                        .iter()
                        .map(|part| transcription_challenge::PartGradedPart {
                            heard: part.clone(),
                            grade: transcription_challenge::WordGrade::Perfect {
                                wrote: Some(part.word.text.clone()),
                            },
                        })
                        .collect();
                    transcription_challenge::PartGraded::AskedToTranscribe {
                        parts,
                        submission: submission.clone(),
                    }
                }
                transcription_challenge::PartSubmitted::Provided { part } => {
                    transcription_challenge::PartGraded::Provided { part }
                }
            })
            .collect();

        return Ok(transcription_challenge::Grade {
            encouragement: Some("Perfect! You transcribed everything correctly!".to_string()),
            explanation: None,
            results,
            compare: Vec::new(),
            autograding_error: None,
        });
    }

    let request = autograde::AutoGradeTranscriptionRequest { submission, course };

    let response = hit_ai_server(
        fetch_happen::Method::POST,
        "/autograde-transcription",
        Some(&request),
        access_token.as_ref(),
    )
    .await
    .map_err(|e| JsValue::from_str(&format!("Request error: {e:?}")))?;

    let response: transcription_challenge::Grade = response
        .json()
        .await
        .map_err(|e| JsValue::from_str(&format!("Response parsing error: {e:?}")))?;

    Ok(response)
}

fn remove_accents(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_courses() -> Vec<language_utils::Course> {
    language_utils::COURSES.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Days;

    impl Default for Deck {
        fn default() -> Self {
            // Read the French language data from file for tests
            // Vec<u8> provides proper alignment for rkyv deserialization
            let bytes = std::fs::read("../out/fra_for_eng/language_data.rkyv")
                .expect("Failed to read test language data");

            let archived = rkyv::access::<
                language_utils::language_pack::ArchivedLanguagePack,
                rkyv::rancor::Error,
            >(&bytes)
            .unwrap();
            let language_pack: LanguagePack =
                rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived).unwrap();

            let language_pack = Arc::new(language_pack);

            let context = Context {
                language_pack,
                course: Course {
                    target_language: Language::French,
                    native_language: Language::English,
                },
            };
            let state = DeckState::new();
            <Deck as weapon::AppState>::finalize(state, &context)
        }
    }

    #[test]
    fn test_fsrs() {
        use chrono::Utc;
        use rs_fsrs::{Card, FSRS, Rating};

        let fsrs = FSRS::default();
        let card = Card::new(Utc::now());

        let record_log = fsrs.repeat(card, Utc::now());
        for rating in Rating::iter() {
            let item = record_log[rating].to_owned();

            println!("{rating:#?}: {item:#?}");

            let record_log = fsrs.repeat(
                item.card,
                Utc::now().checked_add_days(Days::new(10)).unwrap(),
            );

            {
                // For any rating (Easy, Good, Hard, Again), you can compute the new card stats, which includes the next time the card should be reviewed
                let item = record_log[rating].to_owned();

                /* item = SchedulingInfo {
                    card: Card {
                        due: 2025-09-16T18:51:25.591443Z,
                        stability: 104.27451175337288,
                        difficulty: 2.24267983513529,
                        elapsed_days: 10,
                        scheduled_days: 104,
                        reps: 2,
                        lapses: 0,
                        state: Review,
                        last_review: 2025-06-04T18:51:25.591443Z,
                    },
                    review_log: ReviewLog {
                        rating: Easy,
                        elapsed_days: 10,
                        scheduled_days: 15,
                        state: Review,
                        reviewed_date: 2025-06-04T18:51:25.591443Z,
                    },
                } */
                println!("{rating:#?}+{rating:#?}: {item:#?}");
            }
        }
    }

    #[test]
    fn test_card_accumulated_surprise_after_one_easy_review() {
        use chrono::Utc;
        use rs_fsrs::{Card, FSRS, Rating};

        let fsrs = FSRS::default();
        let card = Card::new(Utc::now());

        // Do one easy review
        let record_log = fsrs.repeat(card, Utc::now());
        let after_easy = record_log[&Rating::Easy].to_owned();

        // Easy review should increase positive surprise
        assert!(
            after_easy.card.accumulated_positive_surprise > 0.0,
            "Accumulated positive surprise {} should be greater than 0 after easy review",
            after_easy.card.accumulated_positive_surprise
        );

        // Negative surprise should remain at 0 for easy review
        assert_eq!(
            after_easy.card.accumulated_negative_surprise, 0.0,
            "Accumulated negative surprise should be 0 after easy review"
        );

        println!(
            "✓ After one easy review - Positive surprise: {}, Negative surprise: {}",
            after_easy.card.accumulated_positive_surprise,
            after_easy.card.accumulated_negative_surprise
        );
    }

    #[test]
    fn test_card_accumulated_surprise_after_one_again_review() {
        use chrono::Utc;
        use rs_fsrs::{Card, FSRS, Rating};

        let fsrs = FSRS::default();
        let card = Card::new(Utc::now());

        // Do one "again" review (failed on first attempt)
        let record_log = fsrs.repeat(card, Utc::now());
        let after_again = record_log[&Rating::Again].to_owned();

        // Failed review should only have negative surprise
        assert_eq!(
            after_again.card.accumulated_positive_surprise, 0.0,
            "Positive surprise should be 0 after initial again review"
        );

        assert!(
            after_again.card.accumulated_negative_surprise > 0.0,
            "Negative surprise {} should be greater than 0 after again review",
            after_again.card.accumulated_negative_surprise
        );

        println!(
            "✓ After one again review - Positive surprise: {}, Negative surprise: {}",
            after_again.card.accumulated_positive_surprise,
            after_again.card.accumulated_negative_surprise
        );
        println!("  Lapses: {}", after_again.card.lapses);
    }

    #[test]
    fn test_card_accumulated_surprise_after_two_good_reviews() {
        use chrono::{Days, Utc};
        use rs_fsrs::{Card, FSRS, Rating};

        let fsrs = FSRS::default();
        let mut card = Card::new(Utc::now());

        // Do first good review
        let record_log = fsrs.repeat(card, Utc::now());
        card = record_log[&Rating::Good].card.clone();
        let pos_surprise_first = card.accumulated_positive_surprise;
        let neg_surprise_first = card.accumulated_negative_surprise;

        // Do second good review after 2 weeks
        let review_time = Utc::now().checked_add_days(Days::new(14)).unwrap();
        let record_log = fsrs.repeat(card, review_time);
        card = record_log[&Rating::Good].card.clone();
        let pos_surprise_second = card.accumulated_positive_surprise;
        let neg_surprise_second = card.accumulated_negative_surprise;

        println!("✓ Accumulated surprise progression with two good reviews:");
        println!(
            "  After 1st good - Positive: {pos_surprise_first}, Negative: {neg_surprise_first}"
        );
        println!(
            "  After 2nd good - Positive: {pos_surprise_second}, Negative: {neg_surprise_second}"
        );
        println!(
            "  Positive change: {}",
            pos_surprise_second - pos_surprise_first
        );
        println!(
            "  Negative change: {}",
            neg_surprise_second - neg_surprise_first
        );
        println!("  Reps: {}, Lapses: {}", card.reps, card.lapses);

        // Good reviews typically shouldn't generate much surprise in either direction
        // But the exact behavior depends on FSRS implementation
        println!("  (Good reviews are neutral, surprise accumulation depends on expectations)");
    }

    #[test]
    fn test_card_accumulated_surprise_after_one_easy_and_three_good_reviews() {
        use chrono::{Days, Utc};
        use rs_fsrs::{Card, FSRS, Rating};

        let fsrs = FSRS::default();
        let mut card = Card::new(Utc::now());

        // Do one easy review
        let record_log = fsrs.repeat(card, Utc::now());
        card = record_log[&Rating::Easy].card.clone();
        let pos_surprise_after_easy = card.accumulated_positive_surprise;
        let neg_surprise_after_easy = card.accumulated_negative_surprise;

        // Do three good reviews
        for i in 1..=3 {
            let review_time = Utc::now().checked_add_days(Days::new(i * 14)).unwrap();
            let record_log = fsrs.repeat(card, review_time);
            card = record_log[&Rating::Good].card.clone();
        }

        // Check accumulated surprise after mixed reviews
        println!("✓ Accumulated surprise after 1 easy + 3 good reviews:");
        println!(
            "  Positive: {} (started at {})",
            card.accumulated_positive_surprise, pos_surprise_after_easy
        );
        println!(
            "  Negative: {} (started at {})",
            card.accumulated_negative_surprise, neg_surprise_after_easy
        );
        println!("  Reps: {}, Lapses: {}", card.reps, card.lapses);

        // Easy review should have added positive surprise, good reviews might add less
        assert!(
            card.accumulated_positive_surprise >= pos_surprise_after_easy,
            "Positive surprise should not decrease with successful reviews"
        );
    }

    #[test]
    fn test_card_accumulated_surprise_after_one_easy_and_one_again_review() {
        use chrono::{Days, Utc};
        use rs_fsrs::{Card, FSRS, Rating};

        let fsrs = FSRS::default();
        let mut card = Card::new(Utc::now());

        // Do one easy review
        let record_log = fsrs.repeat(card, Utc::now());
        card = record_log[&Rating::Easy].card.clone();
        let pos_surprise_after_easy = card.accumulated_positive_surprise;
        let neg_surprise_after_easy = card.accumulated_negative_surprise;

        // Do one "again" review (failed review)
        let review_time = Utc::now().checked_add_days(Days::new(14)).unwrap();
        let record_log = fsrs.repeat(card, review_time);
        card = record_log[&Rating::Again].card.clone();

        // Check that negative surprise increased after the "again" review
        assert!(
            card.accumulated_negative_surprise > neg_surprise_after_easy,
            "Negative surprise {} should increase from {} after an 'again' review",
            card.accumulated_negative_surprise,
            neg_surprise_after_easy
        );

        println!("✓ Accumulated surprise after 1 easy + 1 again review:");
        println!(
            "  Positive: {} (was {} after easy)",
            card.accumulated_positive_surprise, pos_surprise_after_easy
        );
        println!(
            "  Negative: {} (was {} after easy)",
            card.accumulated_negative_surprise, neg_surprise_after_easy
        );
        println!("  Lapses: {}", card.lapses);
    }

    #[test]
    fn test_default_deck_creation() {
        use crate::Deck;

        // Test that we can create a default Deck
        let _deck = Deck::default();

        println!("✓ Default Deck created successfully");
    }

    #[test]
    fn test_default_deck_can_add_cards() {
        use crate::{Deck, DeckState};
        use weapon::AppState;

        let mut deck = Deck::default();

        // Test that we can add cards to the default deck
        if let Some(event) = deck.add_next_unknown_cards(None, 1, Vec::new()) {
            let ts = weapon::data_model::Timestamped {
                timestamp: chrono::Utc::now(),
                within_device_events_index: 0,
                event,
            };
            let context = deck.context.clone();
            let state = DeckState::from(deck);
            let state = Deck::process_event(state, &context, &ts);
            deck = Deck::finalize(state, &context);

            // If language pack has data, we should have added a card
            if !context.language_pack.gram_frequencies.is_empty() {
                assert!(!deck.cards.is_empty());
                println!("✓ Successfully added card to default deck");
            } else {
                println!("✓ Language pack is empty, no cards to add (expected)");
            }
        } else {
            println!("✓ No cards available to add (empty language pack)");
        }
    }

    #[test]
    fn test_add_card_limits_scale_with_deck_size() {
        use crate::{Deck, DeckState};
        use weapon::AppState;
        use weapon::data_model::Timestamped;

        let mut deck = Deck::default();

        let assert_limits = |deck: &Deck| {
            let options = deck.add_card_options(Vec::new());
            let expected_max = if deck.num_cards() < 5 {
                1
            } else if deck.num_cards() < 11 {
                2
            } else {
                5
            } as u32;

            assert!(options.smart_add <= expected_max);
            assert!(
                options
                    .manual_add
                    .iter()
                    .all(|(count, _)| *count <= expected_max)
            );
        };

        assert_limits(&deck);

        while deck.num_cards() < 12 {
            let Some(event) = deck.add_next_unknown_cards(None, 5, Vec::new()) else {
                break;
            };

            let timestamped = Timestamped {
                timestamp: chrono::Utc::now(),
                within_device_events_index: 0,
                event,
            };

            let previous_cards = deck.num_cards();
            let context = deck.context.clone();
            let state = DeckState::from(deck);
            let state = Deck::process_event(state, &context, &timestamped);
            deck = Deck::finalize(state, &context);
            assert!(
                deck.num_cards() <= previous_cards + 5,
                "deck should not grow by more than the requested amount"
            );

            assert_limits(&deck);
        }
    }

    /// E2E integration test: loads real weapon event data from disk,
    /// replays all events through the state machine, and verifies
    /// the computed deck state is sane.
    #[test]
    fn test_e2e_load_weapon_data_and_compute_state() {
        use std::collections::BTreeMap;
        use weapon::data_model::{EventStore, EventType, Timestamped};
        use weapon::opfs::parse_event_log_records;

        // 1. Load language pack from rkyv
        let bytes = std::fs::read("../out/fra_for_eng/language_data.rkyv")
            .expect("Failed to read language data - run `cargo run --bin generate-data` first");
        let archived = rkyv::access::<
            language_utils::language_pack::ArchivedLanguagePack,
            rkyv::rancor::Error,
        >(&bytes)
        .unwrap();
        let language_pack: LanguagePack =
            rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived).unwrap();
        let language_pack = Arc::new(language_pack);

        // 2. Set up the EventStore (same as production: EventStore<String, String>)
        let mut store: EventStore<String, String> = EventStore::default();

        // Initialize the streams with the correct typed stores
        store.get_or_insert_default::<EventType<DeckEvent>>("reviews".to_string(), None);
        store.get_or_insert_default::<EventType<DeckSelectionEvent>>(
            "deck_selection".to_string(),
            None,
        );

        // 3. Load and parse the reviews event blob
        let reviews_blob = std::fs::read(
            "test-data/.weapon/user-events/user__aa6b6044-10d0-444b-8518-3696a15d2392/stream__reviews/events.blob",
        )
        .expect("Failed to read reviews events blob");
        let review_records = parse_event_log_records(&reviews_blob);
        println!("Parsed {} review event records", review_records.len());
        assert!(
            !review_records.is_empty(),
            "Expected review events in test data"
        );

        // Group events by device and add them to the store
        let mut reviews_by_device: BTreeMap<String, Vec<Timestamped<serde_json::Value>>> =
            BTreeMap::new();
        for record in &review_records {
            reviews_by_device
                .entry(record.device_id.clone())
                .or_default()
                .push(record.event.clone());
        }
        for (device_id, events) in reviews_by_device {
            let added = store.add_device_events_jsons(
                "reviews".to_string(),
                device_id.clone(),
                events.clone(),
                None,
            );
            println!("Added {added} review events for device {device_id}");
            assert!(added > 0, "Expected to add review events for {device_id}");
        }

        // 4. Load and parse the deck_selection event blob
        let deck_selection_blob = std::fs::read(
            "test-data/.weapon/user-events/user__aa6b6044-10d0-444b-8518-3696a15d2392/stream__deck_selection/events.blob",
        )
        .expect("Failed to read deck_selection events blob");
        let deck_selection_records = parse_event_log_records(&deck_selection_blob);
        println!(
            "Parsed {} deck_selection event records",
            deck_selection_records.len()
        );

        let mut selections_by_device: BTreeMap<String, Vec<Timestamped<serde_json::Value>>> =
            BTreeMap::new();
        for record in &deck_selection_records {
            selections_by_device
                .entry(record.device_id.clone())
                .or_default()
                .push(record.event.clone());
        }
        for (device_id, events) in selections_by_device {
            store.add_device_events_jsons("deck_selection".to_string(), device_id, events, None);
        }

        // 5. Compute the deck state by replaying all events
        let context = Context {
            language_pack,
            course: Course {
                target_language: Language::French,
                native_language: Language::English,
            },
        };
        let initial_state = DeckState::new();
        let stream = store
            .get::<EventType<DeckEvent>>("reviews".to_string())
            .expect("reviews stream should exist");
        let deck: Deck = stream.state(initial_state, &context);

        // 6. Verify the computed state looks reasonable
        let num_cards = deck.num_cards();
        let total_reviews = deck.stats.total_reviews;
        println!("Computed deck state:");
        println!("  Total tracked cards: {num_cards}");
        println!("  Total reviews: {total_reviews}");
        println!("  XP: {}", deck.stats.xp);
        println!(
            "  Has placement test: {}",
            deck.placement_test_results.is_some()
        );
        println!("  Leeches: {}", deck.leeches.len());
        println!("  Start time: {:?}", deck.stats.start_time);

        assert!(num_cards > 0, "Expected cards after replaying events");
        assert!(
            total_reviews > 0,
            "Expected total_reviews > 0 after replaying events"
        );
        assert!(
            deck.stats.xp > 0.0,
            "Expected XP > 0 after replaying events"
        );
        assert!(
            deck.stats.start_time.is_some(),
            "Expected start_time to be set"
        );
    }

    #[test]
    fn test_savoir_sentence_cleanup_and_lookup() {
        // Load language pack
        let bytes = std::fs::read("../out/fra_for_eng/language_data.rkyv")
            .expect("Failed to read language data - run `cargo run --bin generate-data` first");
        let archived = rkyv::access::<
            language_utils::language_pack::ArchivedLanguagePack,
            rkyv::rancor::Error,
        >(&bytes)
        .unwrap();
        let language_pack: LanguagePack =
            rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived).unwrap();

        // The sentence from v1 events (without proper French punctuation spacing)
        let raw_sentence = "Qu'est-ce que tu veux savoir?";

        // The raw sentence should NOT be in the language pack
        let raw_in_rodeo = language_pack.string_rodeo.get(raw_sentence).is_some();
        println!("Raw sentence '{raw_sentence}' in string_rodeo: {raw_in_rodeo}");
        assert!(
            !raw_in_rodeo,
            "Raw sentence should NOT be in language pack (it lacks proper French spacing)"
        );

        // After cleanup, it should match the language pack
        let cleaned_sentence = language_utils::text_cleanup::cleanup_sentence(
            raw_sentence.to_string(),
            Language::French,
        );
        println!("Cleaned sentence: '{cleaned_sentence}'");

        // The cleaned sentence should be in all structures
        let cleaned_in_rodeo = language_pack.string_rodeo.get(&cleaned_sentence).is_some();
        let cleaned_in_encoded = language_pack
            .string_rodeo
            .get(&cleaned_sentence)
            .and_then(|spur| language_pack.encoded_sentences.get(&spur))
            .is_some();
        let cleaned_has_literals = language_pack
            .string_rodeo
            .get(&cleaned_sentence)
            .and_then(|spur| language_pack.sentence_to_literals(&spur, Language::French))
            .is_some();

        println!("Cleaned sentence in string_rodeo: {cleaned_in_rodeo}");
        println!("Cleaned sentence in encoded_sentences: {cleaned_in_encoded}");
        println!("Cleaned sentence has literals: {cleaned_has_literals}");

        assert!(
            cleaned_in_rodeo,
            "Cleaned sentence should be in string_rodeo"
        );
        assert!(
            cleaned_in_encoded,
            "Cleaned sentence should be in encoded_sentences"
        );
        assert!(
            cleaned_has_literals,
            "Cleaned sentence should produce literals"
        );
    }
}
