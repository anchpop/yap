mod audio;
mod deck_selection;
mod directories;
mod language_pack;
mod next_cards;
mod notifications;
pub mod opfs_test;
mod simulation;
mod supabase;
mod utils;

use chrono::{DateTime, Utc};
use deck_selection::DeckSelectionEvent;
use futures::StreamExt;
use imdex_map::IndexMap;
use language_utils::ConsolidatedLanguageDataWithCapacity;
use language_utils::Language;
use language_utils::Literal;
use language_utils::TtsProvider;
use language_utils::TtsRequest;
use language_utils::autograde;
use language_utils::transcription_challenge;
use language_utils::{
    DictionaryEntry, FrequencyEntry, Heteronym, Lexeme, PhrasebookEntry, TargetToNativeWord,
};
use lasso::Spur;
use opfs::persistent::{self};
use rs_fsrs::{FSRS, Rating};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::LazyLock;
use wasm_bindgen::prelude::*;
use weapon::data_model::Event;
use weapon::data_model::{EventStore, EventType, ListenerKey, Timestamped};

use crate::deck_selection::DeckSelection;
use crate::directories::Directories;
use crate::utils::hit_ai_server;
pub use next_cards::NextCardsIterator;

#[wasm_bindgen]
pub struct Weapon {
    // todo: move these into a type in `weapon`
    // btw, we should never hold a borrow across an .await. by avoiding this, we guarantee the absence of "borrow while locked" panics
    store: RefCell<EventStore<String, String>>,
    user_id: Option<String>,
    device_id: String,

    // not this ofc
    language_pack: RefCell<BTreeMap<Language, Arc<LanguagePack>>>,
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
            .map(|s| s.state(DeckSelection::NoneSelected))
    }

    pub async fn get_deck_state(
        &self,
        target_language: Language,
    ) -> Result<Deck, persistent::Error> {
        let language_pack = self.get_language_pack(target_language).await?;

        let initial_deck_state = Deck {
            cards: IndexMap::new(),
            sentences_reviewed: BTreeMap::new(),
            words_listened_to: BTreeMap::new(),
            fsrs: FSRS::new(rs_fsrs::Parameters {
                request_retention: 0.7, // target a 70% chance of forgetting
                ..Default::default()
            }),
            total_reviews: 0,
            daily_streak: None,
            language_pack,
            target_language,
        };
        let store = self.store.borrow_mut();
        let Some(stream) = store.get::<EventType<DeckEvent>>("reviews".to_string()) else {
            return Ok(initial_deck_state);
        };
        Ok(stream.state(initial_deck_state))
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
                &self.device_id,
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
            &self.directories.user_directory_handle,
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
            &self.directories.user_directory_handle,
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
                &user_id,
                &self.device_id,
                Some(stream_id.clone()),
                modifier,
            )
            .await?;
            if supabase_sync_result.downloaded_from_supabase > 0 {
                EventStore::save_to_local_storage(
                    &self.store,
                    &self.directories.user_directory_handle,
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
            &self.directories.user_directory_handle,
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
        let event =
            <Timestamped<EventType<DeckEvent>> as weapon::data_model::Event>::from_json(&event)
                .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        self.store
            .borrow_mut()
            .add_device_event(stream_id, device_id, event, None);
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
    pub async fn cache_language_pack(&self, language: Language) {
        let _ = self.get_language_pack(language).await;
    }
}

impl Weapon {
    pub async fn get_language_pack(
        &self,
        language: Language,
    ) -> Result<Arc<LanguagePack>, persistent::Error> {
        let language_pack = if let Some(language_pack) = self.language_pack.borrow().get(&language)
        {
            language_pack.clone()
        } else {
            let language_pack = language_pack::get_language_pack(
                &self.directories.data_directory_handle,
                language,
                &|_| {},
            )
            .await?;
            self.language_pack
                .borrow_mut()
                .insert(language, Arc::new(language_pack));

            self.language_pack
                .borrow()
                .get(&language)
                .expect("language pack must exist as we just added it")
                .clone()
        };
        Ok(language_pack)
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
pub struct TranslateComprehensibleSentence<S> {
    audio: AudioRequest,
    target_language: S,
    target_language_literals: Vec<Literal<S>>,
    primary_expression: Lexeme<S>,
    unique_target_language_lexemes: Vec<Lexeme<S>>,
    unique_target_language_lexeme_definitions: Vec<(Lexeme<S>, Vec<TargetToNativeWord>)>,
    native_translations: Vec<S>,
}

impl TranslateComprehensibleSentence<Spur> {
    fn resolve(&self, rodeo: &lasso::RodeoReader) -> TranslateComprehensibleSentence<String> {
        TranslateComprehensibleSentence {
            audio: self.audio.clone(),
            target_language: rodeo.resolve(&self.target_language).to_string(),
            target_language_literals: self
                .target_language_literals
                .iter()
                .map(|l| l.resolve(rodeo))
                .collect(),
            primary_expression: self.primary_expression.resolve(rodeo),
            unique_target_language_lexemes: self
                .unique_target_language_lexemes
                .iter()
                .map(|l| l.resolve(rodeo))
                .collect(),
            unique_target_language_lexeme_definitions: self
                .unique_target_language_lexeme_definitions
                .iter()
                .map(|(l, d)| (l.resolve(rodeo), d.clone()))
                .collect(),
            native_translations: self
                .native_translations
                .iter()
                .map(|t| rodeo.resolve(t).to_string())
                .collect(),
        }
    }
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TranscribeComprehensibleSentence<S> {
    target_language: S,
    audio: AudioRequest,
    native_language: S,
    parts: Vec<transcription_challenge::Part>,
}

impl TranscribeComprehensibleSentence<Spur> {
    fn resolve(&self, rodeo: &lasso::RodeoReader) -> TranscribeComprehensibleSentence<String> {
        TranscribeComprehensibleSentence {
            target_language: rodeo.resolve(&self.target_language).to_string(),
            audio: self.audio.clone(),
            native_language: rodeo.resolve(&self.native_language).to_string(),
            parts: self.parts.clone(),
        }
    }
}

#[derive(Debug)]
pub struct LanguagePack {
    rodeo: lasso::RodeoReader,
    translations: HashMap<Spur, Vec<Spur>>,
    words_to_heteronyms: HashMap<Spur, BTreeSet<Heteronym<Spur>>>,
    sentences_containing_lexeme_index: HashMap<Lexeme<Spur>, Vec<Spur>>,
    sentences_to_literals: HashMap<Spur, Vec<Literal<Spur>>>,
    sentences_to_lexemes: HashMap<Spur, Vec<Lexeme<Spur>>>,
    sentences_to_all_lexemes: HashMap<Spur, Vec<Lexeme<Spur>>>,
    word_frequencies: IndexMap<Lexeme<Spur>, FrequencyEntry<Spur>>,
    total_word_count: u64,
    dictionary: BTreeMap<Heteronym<Spur>, DictionaryEntry>,
    phrasebook: BTreeMap<Spur, PhrasebookEntry>,
    word_to_pronunciation: HashMap<Spur, Spur>,
    pronunciation_to_words: HashMap<Spur, Vec<Spur>>,
}

impl LanguagePack {
    fn new(language_data: ConsolidatedLanguageDataWithCapacity) -> Self {
        let rodeo = {
            let rodeo = language_data.intern();
            rodeo.into_reader()
        };

        let sentences: Vec<Spur> = {
            language_data
                .consolidated_language_data
                .target_language_sentences
                .iter()
                .map(|s| rodeo.get(s).unwrap())
                .collect()
        };

        let translations = {
            language_data
                .consolidated_language_data
                .translations
                .iter()
                .map(|(target_language, native_languages)| {
                    (
                        rodeo.get(target_language).unwrap(),
                        native_languages
                            .iter()
                            .map(|n| rodeo.get(n).unwrap())
                            .collect(),
                    )
                })
                .collect()
        };

        let words_to_heteronyms = {
            let mut map: HashMap<Spur, BTreeSet<Heteronym<Spur>>> = HashMap::new();

            for freq in &language_data.consolidated_language_data.frequencies {
                if let Lexeme::Heteronym(heteronym) = &freq.lexeme {
                    let word_spur = rodeo.get(&heteronym.word).unwrap();
                    map.entry(word_spur).or_default().insert({
                        Heteronym {
                            word: rodeo.get(&heteronym.word).unwrap(),
                            lemma: rodeo.get(&heteronym.lemma).unwrap(),
                            pos: heteronym.pos,
                        }
                    });
                }
            }

            map
        };

        let sentences_to_literals = {
            language_data
                .consolidated_language_data
                .nlp_sentences
                .iter()
                .map(|(sentence, analysis)| {
                    (
                        rodeo.get(sentence).unwrap(),
                        analysis
                            .words
                            .iter()
                            .map(|word| word.get_interned(&rodeo).unwrap())
                            .collect(),
                    )
                })
                .collect()
        };

        let sentences_to_lexemes: HashMap<Spur, Vec<Lexeme<Spur>>> = {
            language_data
                .consolidated_language_data
                .nlp_sentences
                .iter()
                .map(|(sentence, analysis)| {
                    (
                        rodeo.get(sentence).unwrap(),
                        analysis
                            .lexemes()
                            .map(|l| l.get_interned(&rodeo).unwrap())
                            .collect(),
                    )
                })
                .collect()
        };

        let sentences_containing_lexeme_index = {
            let mut map = HashMap::new();
            for (i, sentence_spur) in sentences.iter().enumerate() {
                let _sentence = rodeo.resolve(sentence_spur);
                let Some(lexemes) = sentences_to_lexemes.get(sentence_spur) else {
                    continue;
                };
                for lexeme in lexemes.iter().cloned() {
                    map.entry(lexeme).or_insert(vec![]).push(sentences[i]);
                }
            }
            map
        };

        let sentences_to_all_lexemes = {
            language_data
                .consolidated_language_data
                .nlp_sentences
                .iter()
                .map(|(sentence, analysis)| {
                    (
                        rodeo.get(sentence).unwrap(),
                        analysis
                            .all_lexemes()
                            .map(|l| l.get_interned(&rodeo).unwrap())
                            .collect(),
                    )
                })
                .collect()
        };

        let word_frequencies = {
            let mut map = IndexMap::new();
            for freq in &language_data.consolidated_language_data.frequencies {
                map.insert(
                    freq.lexeme.get_interned(&rodeo).unwrap(),
                    freq.get_interned(&rodeo).unwrap(),
                );
            }
            map
        };

        let total_word_count = {
            language_data
                .consolidated_language_data
                .frequencies
                .iter()
                .map(|freq| freq.count as u64)
                .sum()
        };

        let dictionary = {
            language_data
                .consolidated_language_data
                .dictionary
                .iter()
                .map(|(heteronym, entry)| (heteronym.get_interned(&rodeo).unwrap(), entry.clone()))
                .collect()
        };

        let phrasebook = {
            language_data
                .consolidated_language_data
                .phrasebook
                .iter()
                .map(|(multiword_term, entry)| (rodeo.get(multiword_term).unwrap(), entry.clone()))
                .collect()
        };

        let word_to_pronunciation = {
            language_data
                .consolidated_language_data
                .word_to_pronunciation
                .iter()
                .map(|(word, pronunciation)| {
                    (rodeo.get(word).unwrap(), rodeo.get(pronunciation).unwrap())
                })
                .collect()
        };

        let pronunciation_to_words = {
            language_data
                .consolidated_language_data
                .pronunciation_to_words
                .iter()
                .map(|(pronunciation, words)| {
                    (
                        rodeo.get(pronunciation).unwrap(),
                        words.iter().map(|word| rodeo.get(word).unwrap()).collect(),
                    )
                })
                .collect()
        };

        Self {
            rodeo,
            translations,
            words_to_heteronyms,
            sentences_containing_lexeme_index,
            sentences_to_literals,
            sentences_to_lexemes,
            sentences_to_all_lexemes,
            word_frequencies,
            total_word_count,
            dictionary,
            phrasebook,
            word_to_pronunciation,
            pronunciation_to_words,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum SentenceReviewResult {
    Perfect {},
    Wrong {
        submission: String,
        lexemes_remembered: BTreeSet<Lexeme<String>>,
        lexemes_forgotten: BTreeSet<Lexeme<String>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum CardType {
    TargetLanguage,
    Listening,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AddCardOptions {
    pub smart_add: u32,
    pub manual_add: Vec<(u32, CardType)>,
}

#[derive(
    Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify, Hash,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum CardIndicator<S> {
    TargetLanguage { lexeme: Lexeme<S> },
    ListeningHomophonous { pronunciation: S },
}

impl<S> CardIndicator<S> {
    pub fn target_language(&self) -> Option<&Lexeme<S>> {
        match self {
            CardIndicator::TargetLanguage { lexeme } => Some(lexeme),
            _ => None,
        }
    }

    pub fn listening_homophonous(&self) -> Option<&S> {
        match self {
            CardIndicator::ListeningHomophonous { pronunciation } => Some(pronunciation),
            _ => None,
        }
    }
}

impl CardIndicator<String> {
    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<CardIndicator<Spur>> {
        Some(match self {
            CardIndicator::TargetLanguage { lexeme } => CardIndicator::TargetLanguage {
                lexeme: lexeme.get_interned(rodeo)?,
            },
            CardIndicator::ListeningHomophonous { pronunciation } => {
                CardIndicator::ListeningHomophonous {
                    pronunciation: rodeo.get(pronunciation)?,
                }
            }
        })
    }
}

impl CardIndicator<Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> CardIndicator<String> {
        match self {
            CardIndicator::TargetLanguage { lexeme } => CardIndicator::TargetLanguage {
                lexeme: lexeme.resolve(rodeo),
            },
            CardIndicator::ListeningHomophonous { pronunciation } => {
                CardIndicator::ListeningHomophonous {
                    pronunciation: rodeo.resolve(pronunciation).to_string(),
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum SentenceReviewIndicator {
    TargetToNative {
        challenge_sentence: String,
        result: SentenceReviewResult,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LanguageEvent {
    pub language: Language,
    pub content: LanguageEventContent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum LanguageEventContent {
    AddCards {
        cards: Vec<CardIndicator<String>>,
    },
    ReviewCard {
        reviewed: CardIndicator<String>,
        rating: String,
    },
    #[serde(rename = "ReviewSentence")]
    TranslationChallenge {
        review: SentenceReviewIndicator,
    },
    TranscriptionChallenge {
        challenge: Vec<transcription_challenge::PartGraded>,
    },
}

// Event types
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum DeckEvent {
    Language(LanguageEvent),
}
#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "version")]
pub enum VersionedDeckEvent {
    V1(DeckEvent),
}

impl Event for DeckEvent {
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        let versioned = VersionedDeckEvent::from(self.clone());
        serde_json::to_value(versioned)
    }

    fn from_json(json: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value::<VersionedDeckEvent>(json.clone()).map(|versioned| versioned.into())
    }
}
impl From<DeckEvent> for VersionedDeckEvent {
    fn from(event: DeckEvent) -> Self {
        VersionedDeckEvent::V1(event)
    }
}
impl From<VersionedDeckEvent> for DeckEvent {
    fn from(event: VersionedDeckEvent) -> Self {
        match event {
            VersionedDeckEvent::V1(event) => event,
        }
    }
}

#[derive(Clone, Debug)]
struct CardData {
    fsrs_card: rs_fsrs::Card,
}

#[derive(Clone, Debug)]
struct DailyStreak {
    streak_start: chrono::DateTime<chrono::Utc>,
    last_review_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Deck {
    cards: IndexMap<CardIndicator<Spur>, CardData>,
    sentences_reviewed: BTreeMap<Spur, u32>,
    words_listened_to: BTreeMap<Heteronym<Spur>, u32>,

    fsrs: FSRS,
    total_reviews: u64,
    daily_streak: Option<DailyStreak>,

    language_pack: Arc<LanguagePack>,
    target_language: Language,
}

struct ComprehensibleSentence {
    target_language: Spur,
    target_language_literals: Vec<Literal<Spur>>,
    unique_target_language_lexemes: Vec<Lexeme<Spur>>,
    native_languages: Vec<Spur>,
}

impl weapon::AppState for Deck {
    type Event = DeckEvent;

    fn apply_event(mut self, event: &Timestamped<Self::Event>) -> Self {
        let Timestamped::<DeckEvent> {
            event,
            timestamp,
            within_device_events_index: _,
        } = event;

        let DeckEvent::Language(LanguageEvent {
            language: event_language,
            content: event,
        }) = event;

        self.update_daily_streak(timestamp);
        self.total_reviews += 1;

        if *event_language != self.target_language {
            return self;
        }

        match event {
            LanguageEventContent::AddCards { cards } => {
                for card in cards {
                    if let Some(card) = card.get_interned(&self.language_pack.rodeo) {
                        if !self.cards.contains_key(&card) {
                            // Make sure the card is actually in the respective database
                            match &card {
                                CardIndicator::TargetLanguage { lexeme } => {
                                    if !self.language_pack.word_frequencies.contains_key(lexeme) {
                                        continue;
                                    }
                                }
                                CardIndicator::ListeningHomophonous { pronunciation } => {
                                    if !self
                                        .language_pack
                                        .pronunciation_to_words
                                        .contains_key(pronunciation)
                                    {
                                        continue;
                                    }
                                }
                            }

                            self.cards.insert(
                                card.clone(),
                                CardData {
                                    fsrs_card: rs_fsrs::Card::new(),
                                },
                            );
                        }
                    }
                }
            }
            LanguageEventContent::ReviewCard { reviewed, rating } => {
                if let Some(reviewed) = reviewed.get_interned(&self.language_pack.rodeo) {
                    // Log review
                    let rating = match rating.as_str() {
                        "again" => Rating::Again,
                        "hard" => Rating::Hard,
                        "good" => Rating::Good,
                        "easy" => Rating::Easy,
                        _ => return self, // Invalid rating, don't apply
                    };

                    self.log_review(reviewed.clone(), rating, *timestamp);
                }
            }
            LanguageEventContent::TranslationChallenge {
                review:
                    SentenceReviewIndicator::TargetToNative {
                        challenge_sentence,
                        result: SentenceReviewResult::Perfect {},
                    },
            } => {
                if let Some(challenge_sentence) = self.language_pack.rodeo.get(challenge_sentence) {
                    if let Some(lexemes) = self
                        .language_pack
                        .sentences_to_lexemes
                        .get(&challenge_sentence)
                    {
                        let sentence_review_count = self
                            .sentences_reviewed
                            .entry(challenge_sentence)
                            .or_insert(0);
                        *sentence_review_count += 1;

                        let lexemes = lexemes.clone();
                        for lexeme in lexemes {
                            self.log_review(
                                CardIndicator::TargetLanguage { lexeme },
                                Rating::Good,
                                *timestamp,
                            );
                        }
                    }
                }
            }
            LanguageEventContent::TranslationChallenge {
                review:
                    SentenceReviewIndicator::TargetToNative {
                        challenge_sentence: _,
                        result:
                            SentenceReviewResult::Wrong {
                                submission: _,
                                lexemes_remembered,
                                lexemes_forgotten,
                            },
                    },
            } => {
                for lexeme in lexemes_remembered {
                    if let Some(lexeme) = lexeme.get_interned(&self.language_pack.rodeo) {
                        self.log_review(
                            CardIndicator::TargetLanguage { lexeme },
                            Rating::Good,
                            *timestamp,
                        );
                    }
                }

                for lexeme in lexemes_forgotten {
                    if let Some(lexeme) = lexeme.get_interned(&self.language_pack.rodeo) {
                        self.log_review(
                            CardIndicator::TargetLanguage { lexeme },
                            Rating::Again,
                            *timestamp,
                        );
                    }
                }
            }
            LanguageEventContent::TranscriptionChallenge { challenge } => {
                let mut perfect = true;
                // Process each part of the transcription challenge
                for part in challenge {
                    match part {
                        transcription_challenge::PartGraded::AskedToTranscribe {
                            parts, ..
                        } => {
                            // Grade each word that was transcribed
                            for graded_part in parts {
                                // Only process words that have heteronyms (actual vocabulary)
                                if let Some(heteronym) = &graded_part.heard.heteronym
                                    && let Some(heteronym) =
                                        heteronym.get_interned(&self.language_pack.rodeo)
                                {
                                    let pronunciation = *self
                                        .language_pack
                                        .word_to_pronunciation
                                        .get(&heteronym.word)
                                        .unwrap();
                                    let card =
                                        CardIndicator::ListeningHomophonous { pronunciation };

                                    // Map the grade to a FSRS rating
                                    let rating = match &graded_part.grade {
                                        transcription_challenge::WordGrade::Perfect {} => Rating::Good,
                                        transcription_challenge::WordGrade::CorrectWithTypo {} => Rating::Good,
                                        transcription_challenge::WordGrade::PhoneticallyIdenticalButContextuallyIncorrect {} => Rating::Hard,
                                        transcription_challenge::WordGrade::PhoneticallySimilarButContextuallyIncorrect {} => Rating::Again,
                                        transcription_challenge::WordGrade::Incorrect {} => Rating::Again,
                                        transcription_challenge::WordGrade::Missed {} => Rating::Again,
                                    };

                                    if rating != Rating::Again {
                                        *self.words_listened_to.entry(heteronym).or_insert(0) += 1;
                                    } else {
                                        perfect = false;
                                    }

                                    self.log_review(card, rating, *timestamp);
                                }
                            }
                        }
                        transcription_challenge::PartGraded::Provided { .. } => {
                            // Provided parts don't need grading
                        }
                    }
                }
                if perfect {
                    let challenge_sentence = challenge
                        .iter()
                        .flat_map(|part| match part {
                            transcription_challenge::PartGraded::AskedToTranscribe {
                                parts,
                                ..
                            } => parts
                                .iter()
                                .flat_map(|part| {
                                    vec![part.heard.text.clone(), part.heard.whitespace.clone()]
                                })
                                .collect::<Vec<_>>(),
                            transcription_challenge::PartGraded::Provided { part } => {
                                vec![part.text.clone(), part.whitespace.clone()]
                            }
                        })
                        .collect::<Vec<String>>()
                        .join("");
                    if let Some(challenge_sentence) =
                        self.language_pack.rodeo.get(&challenge_sentence)
                    {
                        let sentence_review_count = self
                            .sentences_reviewed
                            .entry(challenge_sentence)
                            .or_insert(0);
                        *sentence_review_count += 1;
                    }
                }
            }
        }

        self
    }
}

impl Deck {
    fn log_review(
        &mut self,
        card: CardIndicator<Spur>,
        rating: Rating,
        timestamp: DateTime<Utc>,
    ) -> Option<&CardData> {
        let word_frequencies = &self.language_pack.word_frequencies;
        let pronunciation_to_words = &self.language_pack.pronunciation_to_words;

        // Make sure the card is actually in the respective database
        match &card {
            CardIndicator::TargetLanguage { lexeme } => {
                if !word_frequencies.contains_key(lexeme) {
                    return None;
                }
            }
            CardIndicator::ListeningHomophonous { pronunciation } => {
                if !pronunciation_to_words.contains_key(pronunciation) {
                    return None;
                }
            }
        }

        let card_data = self.cards.get_mut(&card)?;
        let record_log = self.fsrs.repeat(card_data.fsrs_card.clone(), timestamp);
        card_data.fsrs_card = record_log[&rating].card.clone();
        Some(card_data)
    }

    fn update_daily_streak(&mut self, timestamp: &DateTime<Utc>) {
        match &self.daily_streak {
            None => {
                // First review ever
                self.daily_streak = Some(DailyStreak {
                    streak_start: *timestamp,
                    last_review_time: *timestamp,
                });
            }
            Some(streak) => {
                if timestamp > &streak.last_review_time {
                    // This is a newer review
                    let hours_since_last = (*timestamp - streak.last_review_time).num_hours();

                    if hours_since_last <= 30 {
                        // Within 30 hours, continue streak
                        self.daily_streak = Some(DailyStreak {
                            streak_start: streak.streak_start,
                            last_review_time: *timestamp,
                        });
                    } else {
                        // More than 30 hours, start new streak
                        self.daily_streak = Some(DailyStreak {
                            streak_start: *timestamp,
                            last_review_time: *timestamp,
                        });
                    }
                }
                // If timestamp <= last_review_time, it's an old event being processed, ignore
            }
        }
    }

    fn get_card(&self, index: usize) -> Option<(CardIndicator<Spur>, Card)> {
        let (card_indicator, card_data) = self.cards.get_index(index)?;

        let card = Card {
            content: match card_indicator {
                CardIndicator::TargetLanguage {
                    lexeme: Lexeme::Heteronym(heteronym),
                } => {
                    let Some(entry) = self.language_pack.dictionary.get(heteronym).cloned() else {
                        panic!(
                            "Heteronym {heteronym:?} was in the deck, but was not found in dictionary"
                        );
                    };
                    CardContent::Heteronym(*heteronym, entry.definitions.clone())
                }
                CardIndicator::TargetLanguage {
                    lexeme: Lexeme::Multiword(multiword_term),
                } => {
                    let Some(entry) = self.language_pack.phrasebook.get(multiword_term).cloned()
                    else {
                        panic!(
                            "Multiword term {multiword_term:?} was in the deck, but was not found in phrasebook"
                        );
                    };
                    CardContent::Multiword(
                        *multiword_term,
                        MultiwordCardContent {
                            meaning: entry.meaning.clone(),
                            example_sentence_target_language: entry.target_language_example.clone(),
                            example_sentence_native_language: entry.native_language_example.clone(),
                        },
                    )
                }
                CardIndicator::ListeningHomophonous { pronunciation } => {
                    let Some(possible_words) = self
                        .language_pack
                        .pronunciation_to_words
                        .get(pronunciation)
                        .cloned()
                    else {
                        panic!(
                            "Pronunciation {pronunciation:?} was in the deck, but was not found in pronunciation_to_words"
                        );
                    };
                    let possible_words = possible_words.into_iter().collect::<BTreeSet<_>>();

                    let words_known = self
                        .cards
                        .keys()
                        .filter_map(CardIndicator::target_language)
                        .filter_map(Lexeme::heteronym)
                        .map(|heteronym| heteronym.word)
                        .collect::<BTreeSet<_>>();
                    // figure out which of those words the user knows
                    let possible_words = possible_words
                        .iter()
                        .map(|word| (words_known.contains(word), *word))
                        .collect();
                    CardContent::Listening {
                        pronunciation: *pronunciation,
                        possible_words,
                    }
                }
            },
            fsrs_card: card_data.fsrs_card.clone(),
        };
        Some((card_indicator.clone(), card))
    }

    fn get_comprehensible_sentence_containing(
        &self,
        required_lexeme: &Lexeme<Spur>,
        sentences_reviewed: &BTreeMap<Spur, u32>,
        language_pack: &LanguagePack,
    ) -> Option<ComprehensibleSentence> {
        // Get all words that are in "review" state or the target word
        let mut comprehensible_words: BTreeSet<Lexeme<Spur>> = self
            .cards
            .iter()
            .filter_map(|(card_indicator, card_data)| match card_indicator {
                CardIndicator::TargetLanguage { lexeme } => Some((lexeme, card_data)),
                CardIndicator::ListeningHomophonous { .. } => None,
            })
            .filter(|(_, card_data)| matches!(card_data.fsrs_card.state, rs_fsrs::State::Review))
            .map(|(target_language_word, _)| *target_language_word)
            .collect();

        // Add the target word to comprehensible words
        comprehensible_words.insert(*required_lexeme);

        // Search through all sentences
        let candidate_sentences = language_pack
            .sentences_containing_lexeme_index
            .get(required_lexeme)?;

        let mut possible_sentences = Vec::new();

        // Warning: this loop is HOT!
        'checkSentences: for sentence in candidate_sentences {
            let Some(lexemes) = language_pack.sentences_to_all_lexemes.get(sentence) else {
                continue;
            };

            for lexeme in lexemes {
                if !comprehensible_words.contains(lexeme) {
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
            let target_language = **possible_sentences.first()?;

            let lexemes = language_pack
                .sentences_to_all_lexemes
                .get(&target_language)?;

            let unique_target_language_lexemes = {
                let mut unique_target_language_lexemes = vec![];
                let mut lexemes_set = BTreeSet::new();

                for lexeme in lexemes {
                    if !lexemes_set.contains(&lexeme) {
                        unique_target_language_lexemes.push(*lexeme);
                        lexemes_set.insert(lexeme);
                    }
                }
                unique_target_language_lexemes
            };

            let native_languages = language_pack
                .translations
                .get(&target_language)
                .unwrap()
                .clone();

            let target_language_literals = language_pack
                .sentences_to_literals
                .get(&target_language)
                .unwrap()
                .clone();

            return Some(ComprehensibleSentence {
                target_language,
                target_language_literals,
                unique_target_language_lexemes,
                native_languages,
            });
        }

        None
    }

    pub fn next_unknown_cards(&self, card_type: Option<CardType>) -> NextCardsIterator<'_> {
        let permitted_types = match card_type {
            Some(CardType::TargetLanguage) => vec![ChallengeType::Text],
            Some(CardType::Listening) => vec![ChallengeType::Listening],
            None => vec![ChallengeType::Text, ChallengeType::Listening],
        };
        NextCardsIterator::new(self, permitted_types)
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Deck {
    /// First, the frontend calls get_all_cards_summary to get a view of what cards are due and what cards are going to be due in the future.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_all_cards_summary(&self) -> Vec<CardSummary> {
        let language_pack: &LanguagePack = &self.language_pack;
        let rodeo = &language_pack.rodeo;
        let mut summaries: Vec<CardSummary> = self
            .cards
            .iter()
            .map(|(card_indicator, card_data)| CardSummary {
                card_indicator: card_indicator.resolve(rodeo),
                due_timestamp_ms: card_data.fsrs_card.due.timestamp_millis() as f64,
                state: match card_data.fsrs_card.state {
                    rs_fsrs::State::New => "new".to_string(),
                    rs_fsrs::State::Learning => "learning".to_string(),
                    rs_fsrs::State::Review => "review".to_string(),
                    rs_fsrs::State::Relearning => "relearning".to_string(),
                },
            })
            .collect();

        // Sort by due date
        summaries.sort_by(|a, b| a.due_timestamp_ms.partial_cmp(&b.due_timestamp_ms).unwrap());

        summaries
    }

    /// TODO: get_review_info and get_all_cards_summary can probably be combined.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_review_info(&self, banned_challenge_types: Vec<ChallengeType>) -> ReviewInfo {
        let now = Utc::now();
        let mut due_cards = vec![];
        let mut future_cards = vec![];
        let mut due_but_banned_cards = vec![];

        let no_listening_cards = banned_challenge_types.contains(&ChallengeType::Listening);
        let no_text_cards = banned_challenge_types.contains(&ChallengeType::Text);

        for (index, (card, card_data)) in self.cards.iter().enumerate() {
            let due_date = card_data.fsrs_card.due;
            if due_date <= now {
                match card {
                    CardIndicator::TargetLanguage { .. } if no_text_cards => {
                        due_but_banned_cards.push(index);
                    }
                    CardIndicator::ListeningHomophonous { .. } if no_listening_cards => {
                        due_but_banned_cards.push(index);
                    }
                    CardIndicator::TargetLanguage { .. }
                    | CardIndicator::ListeningHomophonous { .. } => due_cards.push(index),
                }
            } else {
                future_cards.push(index);
            }
        }

        // sort by due date
        due_cards.sort_by_key(|card_index| {
            let (_, card) = self.get_card(*card_index).unwrap();
            ordered_float::NotNan::new(card.due_timestamp_ms()).unwrap()
        });

        due_but_banned_cards.sort_by_key(|card_index| {
            let (_, card) = self.get_card(*card_index).unwrap();
            ordered_float::NotNan::new(card.due_timestamp_ms()).unwrap()
        });

        future_cards.sort_by_key(|card_index| {
            let (_, card) = self.get_card(*card_index).unwrap();
            ordered_float::NotNan::new(card.due_timestamp_ms()).unwrap()
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

        const SIMULATION_DAYS: u32 = 3;
        let mut requests = Vec::new();
        self.simulate_days(SIMULATION_DAYS, |challenge| {
            requests.push(challenge.audio_request());
        });
        let requests = requests.into_iter();

        let requested_filenames = futures::stream::iter(requests)
            .map(|request| {
                let audio_cache = audio_cache.clone();
                let abort_signal = abort_signal.clone();
                async move {
                    // Check if aborted before processing
                    if let Some(ref signal) = abort_signal {
                        if signal.aborted() {
                            return None;
                        }
                    }

                    // Generate the cache filename for this request
                    let cache_filename =
                        audio::AudioCache::get_cache_filename(&request.request, &request.provider);

                    // Just try to fetch and cache, ignoring errors for individual requests
                    let _ = audio_cache.fetch_and_cache(&request, access_token).await;
                    Some(cache_filename)
                }
            })
            .buffered(3)
            .filter_map(|x| async { x })
            .collect::<BTreeSet<_>>()
            .await;

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
            .cards
            .iter()
            .filter_map(|(card_indicator, card_data)| match card_indicator {
                CardIndicator::TargetLanguage { lexeme } => Some((lexeme, card_data)),
                CardIndicator::ListeningHomophonous { .. } => None,
            })
            .filter_map(|(lexeme, card_data)| {
                if card_data.fsrs_card.state != rs_fsrs::State::New {
                    self.language_pack.word_frequencies.get(lexeme)
                } else {
                    None
                }
            })
            .map(|freq| freq.count as u64)
            .sum();
        total_words_reviewed as f64 / self.language_pack.total_word_count as f64
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_total_reviews(&self) -> u64 {
        self.total_reviews
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_daily_streak(&self) -> u32 {
        match &self.daily_streak {
            None => 0,
            Some(streak) => {
                let now = chrono::Utc::now();
                let hours_since_last = (now - streak.last_review_time).num_hours();

                if hours_since_last <= 30 {
                    // Streak is active (reviewed within last 30 hours)
                    (streak.last_review_time.date_naive() - streak.streak_start.date_naive())
                        .num_days() as u32
                        + 1
                } else {
                    // Streak is broken
                    0
                }
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn add_card_options(&self) -> AddCardOptions {
        AddCardOptions {
            smart_add: self.next_unknown_cards(None).take(5).count() as u32,
            manual_add: vec![
                (
                    self.next_unknown_cards(Some(CardType::TargetLanguage))
                        .take(5)
                        .count() as u32,
                    CardType::TargetLanguage,
                ),
                (
                    self.next_unknown_cards(Some(CardType::Listening))
                        .take(5)
                        .count() as u32,
                    CardType::Listening,
                ),
            ],
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn add_next_unknown_cards(
        &self,
        card_type: Option<CardType>,
        count: usize,
    ) -> Option<DeckEvent> {
        let cards = self
            .next_unknown_cards(card_type)
            .take(count)
            .map(|card| card.resolve(&self.language_pack.rodeo))
            .collect::<Vec<_>>();

        (!cards.is_empty()).then_some({
            DeckEvent::Language(LanguageEvent {
                language: self.target_language,
                content: LanguageEventContent::AddCards { cards },
            })
        })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn review_card(
        &self,
        reviewed: CardIndicator<String>,
        rating: String,
    ) -> Option<DeckEvent> {
        let indicator = reviewed.get_interned(&self.language_pack.rodeo)?;
        self.cards.contains_key(&indicator).then_some({
            DeckEvent::Language(LanguageEvent {
                language: self.target_language,
                content: LanguageEventContent::ReviewCard { reviewed, rating },
            })
        })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn translate_sentence_perfect(&self, challenge_sentence: String) -> Option<DeckEvent> {
        Some(DeckEvent::Language(LanguageEvent {
            language: self.target_language,
            content: LanguageEventContent::TranslationChallenge {
                review: SentenceReviewIndicator::TargetToNative {
                    challenge_sentence,
                    result: SentenceReviewResult::Perfect {},
                },
            },
        }))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn translate_sentence_wrong(
        &self,
        challenge_sentence: String,
        submission: String,
        words_remembered: Vec<Lexeme<String>>,
        words_forgotten: Vec<Lexeme<String>>,
    ) -> Option<DeckEvent> {
        Some(DeckEvent::Language(LanguageEvent {
            language: self.target_language,
            content: LanguageEventContent::TranslationChallenge {
                review: SentenceReviewIndicator::TargetToNative {
                    challenge_sentence,
                    result: SentenceReviewResult::Wrong {
                        submission,
                        lexemes_remembered: words_remembered.into_iter().collect(),
                        lexemes_forgotten: words_forgotten.into_iter().collect(),
                    },
                },
            },
        }))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn transcribe_sentence(
        &self,
        challenge: Vec<transcription_challenge::PartGraded>,
    ) -> Option<DeckEvent> {
        Some(DeckEvent::Language(LanguageEvent {
            language: self.target_language,
            content: LanguageEventContent::TranscriptionChallenge { challenge },
        }))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn num_cards(&self) -> usize {
        self.cards.len()
    }
}

#[derive(Debug)]
pub struct Card {
    content: CardContent<Spur>,
    fsrs_card: rs_fsrs::Card,
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MultiwordCardContent {
    meaning: String,
    example_sentence_target_language: String,
    example_sentence_native_language: String,
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum CardContent<S> {
    Heteronym(Heteronym<S>, Vec<TargetToNativeWord>),
    Multiword(S, MultiwordCardContent),
    Listening {
        pronunciation: S,
        possible_words: Vec<(bool, S)>,
    },
}

impl<S> CardContent<S> {
    fn lexeme(&self) -> Option<Lexeme<S>>
    where
        S: Clone,
    {
        match self {
            CardContent::Heteronym(heteronym, _) => Some(Lexeme::Heteronym(heteronym.clone())),
            CardContent::Multiword(multiword_term, _) => {
                Some(Lexeme::Multiword(multiword_term.clone()))
            }
            CardContent::Listening { .. } => None,
        }
    }

    fn pronunciation(&self) -> Option<S>
    where
        S: Clone,
    {
        match self {
            CardContent::Listening { pronunciation, .. } => Some(pronunciation.clone()),
            _ => None,
        }
    }
}

impl CardContent<Spur> {
    fn resolve(&self, rodeo: &lasso::RodeoReader) -> CardContent<String> {
        match self {
            CardContent::Heteronym(heteronym, definitions) => {
                CardContent::Heteronym(heteronym.resolve(rodeo), definitions.clone())
            }
            CardContent::Multiword(multiword, content) => {
                CardContent::Multiword(rodeo.resolve(multiword).to_string(), content.clone())
            }
            CardContent::Listening {
                pronunciation,
                possible_words,
            } => CardContent::Listening {
                pronunciation: rodeo.resolve(pronunciation).to_string(),
                possible_words: possible_words
                    .iter()
                    .map(|(known, word)| (*known, rodeo.resolve(word).to_string()))
                    .collect(),
            },
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug, Clone)]
pub struct ReviewInfo {
    due_cards: Vec<usize>,
    due_but_banned_cards: Vec<usize>,
    future_cards: Vec<usize>,
}

#[derive(tsify::Tsify, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Challenge<S> {
    FlashCardReview {
        indicator: CardIndicator<S>,
        content: CardContent<S>,
        audio: AudioRequest,
        is_new: bool,
        listening_prefix: Option<String>,
    },
    TranslateComprehensibleSentence(TranslateComprehensibleSentence<S>),
    TranscribeComprehensibleSentence(TranscribeComprehensibleSentence<S>),
}

impl<S> Challenge<S> {
    fn audio_request(&self) -> AudioRequest {
        match self {
            Challenge::FlashCardReview { audio, .. } => audio.clone(),
            Challenge::TranslateComprehensibleSentence(translate_comprehensible_sentence) => {
                translate_comprehensible_sentence.audio.clone()
            }
            Challenge::TranscribeComprehensibleSentence(transcribe_comprehensible_sentence) => {
                transcribe_comprehensible_sentence.audio.clone()
            }
        }
    }
}

impl Challenge<Spur> {
    fn resolve(&self, rodeo: &lasso::RodeoReader) -> Challenge<String> {
        match self {
            Challenge::FlashCardReview {
                indicator,
                content,
                audio,
                is_new,
                listening_prefix,
            } => Challenge::FlashCardReview {
                indicator: indicator.resolve(rodeo),
                content: content.resolve(rodeo),
                audio: audio.clone(),
                is_new: *is_new,
                listening_prefix: listening_prefix.clone(),
            },
            Challenge::TranslateComprehensibleSentence(translate_comprehensible_sentence) => {
                Challenge::TranslateComprehensibleSentence(
                    translate_comprehensible_sentence.resolve(rodeo),
                )
            }
            Challenge::TranscribeComprehensibleSentence(transcribe_comprehensible_sentence) => {
                Challenge::TranscribeComprehensibleSentence(
                    transcribe_comprehensible_sentence.resolve(rodeo),
                )
            }
        }
    }
}

#[derive(tsify::Tsify, Eq, PartialEq, serde::Serialize, serde::Deserialize, Debug, Clone)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum ChallengeType {
    Text,
    Listening,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl ReviewInfo {
    fn get_listening_prefix(language: Language) -> &'static str {
        match language {
            Language::French => "Le mot est",
            Language::Spanish => "La palabra es",
            Language::English => "The word is",
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_challenge_for_card(
        &self,
        deck: &Deck,
        card_index: usize,
    ) -> Option<Challenge<String>> {
        let (card_indicator, card) = deck.get_card(card_index)?;
        let language_pack = &deck.language_pack;

        // If we can't find a suitable challenge, we'll return a flashcard challenge. Let's construct it here
        let listening_prefix = matches!(&card.content, CardContent::Listening { .. })
            .then(|| Self::get_listening_prefix(deck.target_language).to_string());

        let flashcard = Challenge::<Spur>::FlashCardReview {
            audio: match &card.content {
                CardContent::Heteronym(heteronym, _) => AudioRequest {
                    request: TtsRequest {
                        text: language_pack.rodeo.resolve(&heteronym.word).to_string(),
                        language: deck.target_language,
                    },
                    provider: TtsProvider::Google,
                },
                CardContent::Multiword(multiword, _) => AudioRequest {
                    request: TtsRequest {
                        text: language_pack.rodeo.resolve(multiword).to_string(),
                        language: deck.target_language,
                    },
                    provider: TtsProvider::Google,
                },
                CardContent::Listening {
                    pronunciation: _,
                    possible_words,
                } => AudioRequest {
                    request: TtsRequest {
                        text: format!(
                            "{}... \"{}\".",
                            Self::get_listening_prefix(deck.target_language),
                            language_pack.rodeo.resolve(
                                &possible_words
                                    .iter()
                                    .find(|(known, _)| *known)
                                    .or(possible_words.first())
                                    .cloned()
                                    .unwrap()
                                    .1
                            )
                        ),
                        language: deck.target_language,
                    },
                    provider: TtsProvider::Google,
                },
            },
            indicator: card_indicator,
            content: card.content.clone(),
            is_new: card.fsrs_card.state == rs_fsrs::State::New,
            listening_prefix: listening_prefix.clone(),
        };

        let challenge: Challenge<Spur> = if card.fsrs_card.state == rs_fsrs::State::New {
            flashcard
        } else if let Some(pronunciation) = card.content.pronunciation() {
            let known_words: BTreeSet<Lexeme<Spur>> = deck
                .cards
                .keys()
                .filter_map(CardIndicator::target_language)
                .cloned()
                .collect();
            let mut heteronyms = language_pack
                .pronunciation_to_words
                .get(&pronunciation)
                .unwrap()
                .iter()
                .cloned()
                .flat_map(|word| {
                    language_pack
                        .words_to_heteronyms
                        .get(&word)
                        .unwrap()
                        .clone()
                })
                .filter(|heteronym| known_words.contains(&Lexeme::Heteronym(*heteronym)))
                .collect::<Vec<_>>();
            heteronyms.sort_by_key(|heteronym| deck.words_listened_to.get(heteronym).unwrap_or(&0));

            if let Some((target_heteronym, sentence)) = heteronyms
                .iter()
                .filter_map(|heteronym| {
                    let sentence = deck.get_comprehensible_sentence_containing(
                        &Lexeme::Heteronym(*heteronym),
                        &deck.sentences_reviewed,
                        language_pack,
                    )?;
                    Some((*heteronym, sentence))
                })
                .next()
            {
                let parts = sentence
                    .target_language_literals
                    .into_iter()
                    .map(|literal| {
                        if let Some(ref heteronym) = literal.heteronym
                            && heteronym == &target_heteronym
                        {
                            transcription_challenge::Part::AskedToTranscribe {
                                parts: vec![literal.resolve(&language_pack.rodeo)],
                            }
                        } else {
                            transcription_challenge::Part::Provided {
                                part: literal.resolve(&language_pack.rodeo),
                            }
                        }
                    })
                    .collect();
                Challenge::TranscribeComprehensibleSentence(TranscribeComprehensibleSentence {
                    target_language: sentence.target_language,
                    native_language: *sentence.native_languages.first().unwrap(),
                    parts,
                    audio: AudioRequest {
                        request: TtsRequest {
                            text: language_pack
                                .rodeo
                                .resolve(&sentence.target_language)
                                .to_string(),
                            language: deck.target_language,
                        },
                        provider: TtsProvider::ElevenLabs,
                    },
                })
            } else {
                flashcard
            }
        } else if let Some(lexeme) = card.content.lexeme() {
            if let Some(ComprehensibleSentence {
                target_language,
                target_language_literals,
                unique_target_language_lexemes,
                native_languages,
            }) = deck.get_comprehensible_sentence_containing(
                &lexeme,
                &deck.sentences_reviewed,
                language_pack,
            ) {
                let unique_target_language_lexeme_definitions = unique_target_language_lexemes
                    .iter()
                    .map(|lexeme| {
                        let definitions = match lexeme {
                            Lexeme::Heteronym(heteronym) => language_pack
                                .dictionary
                                .get(heteronym)
                                .map(|entry| entry.definitions.clone())
                                .unwrap_or_default(),
                            Lexeme::Multiword(term) => language_pack
                                .phrasebook
                                .get(term)
                                .map(|entry| {
                                    vec![TargetToNativeWord {
                                        native: entry.meaning.clone(),
                                        note: Some(entry.additional_notes.clone()),
                                        example_sentence_target_language: entry
                                            .target_language_example
                                            .clone(),
                                        example_sentence_native_language: entry
                                            .native_language_example
                                            .clone(),
                                    }]
                                })
                                .unwrap_or_default(),
                        };
                        (*lexeme, definitions)
                    })
                    .collect();

                Challenge::TranslateComprehensibleSentence(TranslateComprehensibleSentence {
                    target_language,
                    target_language_literals,
                    unique_target_language_lexemes,
                    native_translations: native_languages,
                    primary_expression: lexeme,
                    unique_target_language_lexeme_definitions,
                    audio: AudioRequest {
                        request: TtsRequest {
                            text: language_pack.rodeo.resolve(&target_language).to_string(),
                            language: deck.target_language,
                        },
                        provider: TtsProvider::ElevenLabs,
                    },
                })
            } else {
                flashcard
            }
        } else {
            flashcard
        };

        Some(challenge.resolve(&language_pack.rodeo))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn get_next_review_card(&self) -> Option<usize> {
        if !self.due_cards.is_empty() {
            Some(self.due_cards[0])
        } else {
            None
        }
    }
}

impl Card {
    pub fn due_timestamp_ms(&self) -> f64 {
        self.fsrs_card.due.timestamp_millis() as f64
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
    card_indicator: CardIndicator<String>,
    due_timestamp_ms: f64,
    state: String,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl CardSummary {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn card_indicator(&self) -> CardIndicator<String> {
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
}

#[wasm_bindgen]
pub fn test_fn(f: js_sys::Function) {
    f.call0(&JsValue::NULL).unwrap();
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
pub async fn autograde_translation(
    challenge_sentence: String,
    user_sentence: String,
    primary_expression: Lexeme<String>,
    lexemes: Vec<Lexeme<String>>,
    access_token: Option<String>,
    language: Language,
) -> Result<autograde::AutoGradeTranslationResponse, JsValue> {
    let request = autograde::AutoGradeTranslationRequest {
        challenge_sentence,
        user_sentence,
        primary_expression: primary_expression.clone(),
        lexemes,
        language,
    };

    let response = hit_ai_server("/autograde-translation", request, access_token.as_ref())
        .await
        .map_err(|e| JsValue::from_str(&format!("Request error: {e:?}")))?;

    if !response.ok() {
        return Err(JsValue::from_str(&format!(
            "HTTP error: {}",
            response.status()
        )));
    }

    let mut response: autograde::AutoGradeTranslationResponse = response
        .json()
        .await
        .map_err(|e| JsValue::from_str(&format!("Response parsing error: {e:?}")))?;

    // make sure the primary expression is in the appropriate array:
    if response.primary_expression_status == autograde::Remembered::Forgot
        && !response.expressions_forgot.contains(&primary_expression)
    {
        response.expressions_forgot.push(primary_expression);
    } else if response.primary_expression_status == autograde::Remembered::Remembered
        && !response
            .expressions_remembered
            .contains(&primary_expression)
    {
        response.expressions_remembered.push(primary_expression);
    }

    log::info!("Autograde response: {response:#?}");

    Ok(response)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn autograde_transcription(
    submission: Vec<transcription_challenge::PartSubmitted>,
    access_token: Option<String>,
    language: Language,
) -> transcription_challenge::Grade {
    let _autograde_error =
        match autograde_transcription_llm(submission.clone(), access_token, language).await {
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
                            let part_text = part.text.to_lowercase().trim().to_string();
                            let submission = submission.to_lowercase().trim().to_string();
                            if part_text == submission {
                                transcription_challenge::PartGradedPart {
                                    heard: part.clone(),
                                    grade: transcription_challenge::WordGrade::Perfect {},
                                }
                            } else if remove_accents(&part_text) == remove_accents(&submission) {
                                transcription_challenge::PartGradedPart {
                                    heard: part.clone(),
                                    grade: transcription_challenge::WordGrade::CorrectWithTypo {},
                                }
                            // todo: check if word entered is in the set of homophones
                            // and if so, grade is as correct PhoneticallyIdenticalButContextuallyIncorrect
                            } else {
                                transcription_challenge::PartGradedPart {
                                    heard: part.clone(),
                                    grade: transcription_challenge::WordGrade::Incorrect {},
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
    language: Language,
) -> Result<transcription_challenge::Grade, JsValue> {
    // Check if all answers are exactly correct (case-insensitive)
    let all_correct = submission.iter().all(|part| match part {
        transcription_challenge::PartSubmitted::AskedToTranscribe { parts, submission } => {
            let submission = submission.trim().to_lowercase();
            let parts = parts
                .iter()
                .map(|part| {
                    format!(
                        "{text}{whitespace}",
                        text = part.text.to_lowercase(),
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
                            grade: transcription_challenge::WordGrade::Perfect {},
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
            explanation: None,
            results,
            compare: Vec::new(),
            autograding_error: None,
        });
    }

    let request = autograde::AutoGradeTranscriptionRequest {
        submission,
        language,
    };

    let response = hit_ai_server("/autograde-transcription", &request, access_token.as_ref())
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
    use chrono::Days;

    #[test]
    fn test_fsrs() {
        use chrono::Utc;
        use rs_fsrs::{Card, FSRS, Rating};

        let fsrs = FSRS::default();
        let card = Card::new();

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
}
