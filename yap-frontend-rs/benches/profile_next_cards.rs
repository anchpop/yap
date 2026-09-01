use std::collections::BTreeMap;
use std::sync::Arc;

use language_utils::language_pack::LanguagePack;
use language_utils::{Course, Language};
use weapon::data_model::{EventStore, EventType, Timestamped};
use weapon::opfs::parse_event_log_records;
use yap_frontend_rs::{Context, Deck, DeckEvent, DeckState};

fn load_deck_from_test_data() -> Deck {
    let language_pack: LanguagePack =
        language_utils::language_pack::load_split_dir(std::path::Path::new("../out/fra_for_eng"))
            .expect("Failed to load language pack - run `cargo run --bin generate-data` first");
    let language_pack = Arc::new(language_pack);

    let mut store: EventStore<String, String> = EventStore::default();
    store.get_or_insert_default::<EventType<DeckEvent>>("reviews".to_string(), None);

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

    let context = Context {
        language_pack,
        course: Course {
            target_language: Language::French,
            native_language: Language::English,
        },
        timezone: chrono::FixedOffset::east_opt(0).unwrap(),
    };
    let initial_state = DeckState::new();
    let stream = store
        .get::<EventType<DeckEvent>>("reviews".to_string())
        .expect("reviews stream should exist");
    stream.state(initial_state, &context)
}

fn main() {
    let deck = load_deck_from_test_data();
    for _ in 0..5000 {
        std::hint::black_box(deck.get_no_cards_ready_info(vec![], None));
    }
}
