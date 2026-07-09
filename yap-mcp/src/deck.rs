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

/// Load the `.rkyv` language pack for a course from the `out/` directory.
pub fn load_language_pack(out_dir: &Path, course: &Course) -> anyhow::Result<Arc<LanguagePack>> {
    let path = out_dir
        .join(format!(
            "{}_for_{}",
            course.target_language.iso_639_3(),
            course.native_language.iso_639_3()
        ))
        .join("language_data.rkyv");
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read language pack at {}", path.display()))?;
    let archived = rkyv::access::<
        language_utils::language_pack::ArchivedLanguagePack,
        rkyv::rancor::Error,
    >(&bytes)
    .map_err(|e| anyhow::anyhow!("failed to access language pack archive: {e}"))?;
    let pack = rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived)
        .map_err(|e| anyhow::anyhow!("failed to deserialize language pack: {e}"))?;
    Ok(Arc::new(pack))
}

/// Replay the reviews stream into a fresh `Deck`.
pub fn build_deck(store: &EventStore<String, String>, context: &Context) -> Deck {
    store
        .get::<EventType<DeckEvent>>(REVIEWS_STREAM.to_string())
        .expect("reviews stream registered in new_store")
        .state(DeckState::new(), context)
}
