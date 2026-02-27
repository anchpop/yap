use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use language_utils::language_pack::LanguagePack;
use language_utils::{Course, Language};
use weapon::data_model::{EventStore, EventType, Timestamped};
use weapon::opfs::parse_event_log_records;
use yap_frontend_rs::{Context, Deck, DeckEvent, DeckState};

fn load_deck_from_test_data() -> Deck {
    let bytes = std::fs::read("../out/fra_for_eng/language_data.rkyv")
        .expect("Failed to read language data - run `cargo run --bin generate-data` first");
    let archived =
        rkyv::access::<language_utils::language_pack::ArchivedLanguagePack, rkyv::rancor::Error>(
            &bytes,
        )
        .unwrap();
    let language_pack: LanguagePack =
        rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived).unwrap();
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
    };
    let initial_state = DeckState::new();
    let stream = store
        .get::<EventType<DeckEvent>>("reviews".to_string())
        .expect("reviews stream should exist");
    stream.state(initial_state, &context)
}

fn main() {
    let deck = load_deck_from_test_data();
    let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    // Measure per-day timing to show scaling behavior
    let mut sim = deck.simulate_usage(fixed_time);
    for day in 0..5 {
        let start = std::time::Instant::now();
        let (next, challenges) = sim.next();
        let elapsed = start.elapsed();
        eprintln!(
            "Day {}: {:>8.2}ms  ({} challenges)",
            day + 1,
            elapsed.as_secs_f64() * 1000.0,
            challenges.len()
        );
        sim = next;
    }

    // Then run a few more iterations for profiling signal
    eprintln!("\nRunning 2 more full 5-day simulations for profiling...");
    for _ in 0..2 {
        let mut sim = deck.simulate_usage(fixed_time);
        for _ in 0..5 {
            let (next, _challenges) = sim.next();
            sim = next;
        }
    }
}
