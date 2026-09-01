//! Building deck state from the weapon event store.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use language_utils::{Course, Language, language_pack::LanguagePack};
use weapon::data_model::{EventStore, EventType, Timestamped};
use yap_frontend_rs::deck_selection::{DeckSelection, DeckSelectionEvent, DeckSelectionPartial};
use yap_frontend_rs::{Context, Deck, DeckEvent, DeckState};

use crate::sync::{DECK_SELECTION_STREAM, EventRow, REVIEWS_STREAM};

/// An event store with the streams the MCP server uses registered.
pub fn new_store() -> EventStore<String, String> {
    let mut store = EventStore::default();
    store.get_or_insert_default::<EventType<DeckEvent>>(REVIEWS_STREAM.to_string(), None);
    store.get_or_insert_default::<EventType<DeckSelectionEvent>>(
        DECK_SELECTION_STREAM.to_string(),
        None,
    );
    store
}

/// Insert fetched rows into the store, grouped per stream/device and ordered
/// by within-device index. The store rejects duplicates and gaps, so echoes of
/// our own uploads are absorbed harmlessly. Returns how many events were
/// actually added.
pub fn insert_rows(store: &mut EventStore<String, String>, rows: Vec<EventRow>) -> usize {
    let mut grouped: BTreeMap<(String, String), Vec<Timestamped<serde_json::Value>>> =
        BTreeMap::new();
    for row in rows {
        if row.stream_id != REVIEWS_STREAM && row.stream_id != DECK_SELECTION_STREAM {
            continue;
        }
        grouped
            .entry((row.stream_id, row.device_id))
            .or_default()
            .push(row.event);
    }
    let mut added = 0;
    for ((stream_id, device_id), mut events) in grouped {
        events.sort_by_key(|e| e.within_device_events_index);
        added += store.add_device_events_jsons(stream_id, device_id, events, None);
    }
    added
}

/// Detect the user's course from their deck_selection events.
pub fn detect_course(store: &EventStore<String, String>) -> anyhow::Result<Course> {
    let selection: DeckSelection = store
        .get::<EventType<DeckSelectionEvent>>(DECK_SELECTION_STREAM.to_string())
        .context("deck_selection stream not registered")?
        .state(
            DeckSelectionPartial {
                target_language: None,
                native_language: None,
                onboarding_selections: BTreeMap::new(),
                selected_languages: BTreeSet::new(),
                heard_about: None,
            },
            &(),
        );

    let target_language = selection
        .target_language
        .context("could not detect a target language from the user's deck_selection events")?;
    let native_language = selection.native_language.unwrap_or(Language::English);
    Ok(Course {
        target_language,
        native_language,
    })
}

/// Lazily-loaded, shared language packs, keyed by course. Loading deserializes
/// a >100 MB rkyv archive, so packs are loaded at most once and shared.
pub struct PackCache {
    out_dir: std::path::PathBuf,
    packs: tokio::sync::Mutex<BTreeMap<Course, Arc<LanguagePack>>>,
}

impl PackCache {
    pub fn new(out_dir: std::path::PathBuf) -> Self {
        PackCache {
            out_dir,
            packs: tokio::sync::Mutex::new(BTreeMap::new()),
        }
    }

    pub async fn get(&self, course: &Course) -> anyhow::Result<Arc<LanguagePack>> {
        let mut packs = self.packs.lock().await;
        if let Some(pack) = packs.get(course) {
            return Ok(pack.clone());
        }
        let out_dir = self.out_dir.clone();
        let course_owned = *course;
        log::info!(
            "loading language pack for {} for {} speakers",
            course.target_language,
            course.native_language
        );
        let pack = tokio::task::spawn_blocking(move || load_language_pack(&out_dir, &course_owned))
            .await??;
        // Same registration the web app does on pack load: lets the shared
        // audio path serve human voice-actor recordings out of this pack.
        yap_frontend_rs::register_human_audio(course.target_language, &pack);
        packs.insert(*course, pack.clone());
        Ok(pack)
    }
}

/// Load the `.rkyv` language pack for a course from the `out/` directory.
pub fn load_language_pack(out_dir: &Path, course: &Course) -> anyhow::Result<Arc<LanguagePack>> {
    let dir = out_dir.join(format!(
        "{}_for_{}",
        course.target_language.code(),
        course.native_language.code()
    ));
    let pack = language_utils::language_pack::load_split_dir(&dir)
        .map_err(|e| anyhow::anyhow!("failed to load language pack in {}: {e}", dir.display()))?;
    Ok(Arc::new(pack))
}

/// Replay the reviews stream into a fresh `Deck`.
pub fn build_deck(store: &EventStore<String, String>, context: &Context) -> Deck {
    store
        .get::<EventType<DeckEvent>>(REVIEWS_STREAM.to_string())
        .expect("reviews stream registered in new_store")
        .state(DeckState::new(), context)
}
