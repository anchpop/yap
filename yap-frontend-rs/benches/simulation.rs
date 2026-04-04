use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use criterion::{Criterion, criterion_group, criterion_main};
use language_utils::language_pack::LanguagePack;
use language_utils::{Course, Language};
use weapon::data_model::{EventStore, EventType, Timestamped};
use weapon::opfs::parse_event_log_records;
use yap_frontend_rs::{Context, Deck, DeckEvent, DeckState};

/// Load the language pack and replay real user events to get a realistic deck.
fn load_deck_from_test_data() -> Deck {
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
    let language_pack = Arc::new(language_pack);

    // Set up event store and replay real events
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

fn bench_simulation(c: &mut Criterion) {
    let deck = load_deck_from_test_data();
    let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    c.bench_function("simulate_1_day", |b| {
        b.iter_batched(
            || deck.simulate_usage(fixed_time),
            |sim| sim.next_day().finish_day(),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("simulate_5_days", |b| {
        b.iter_batched(
            || deck.simulate_usage(fixed_time),
            |mut sim| {
                for _ in 0..5 {
                    sim = sim.next_day().finish_day();
                }
                sim
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_next_unknown_cards(c: &mut Criterion) {
    let deck = load_deck_from_test_data();

    c.bench_function("next_unknown_cards_10", |b| {
        b.iter(|| {
            criterion::black_box(deck.get_no_cards_ready_info(vec![], None));
        })
    });

    c.bench_function("next_unknown_cards_100", |b| {
        b.iter(|| {
            criterion::black_box(deck.get_no_cards_ready_info(vec![], None));
        })
    });
}

criterion_group!(benches, bench_simulation, bench_next_unknown_cards);
criterion_main!(benches);
