//! Link Yap's real exported objects into a native static library. No wrapper state.
pub use yap_frontend_rs::{Deck, Weapon};

use bridgerton::{Error, bridge};

/// Native application startup policy lives here, outside Yap's business logic.
#[bridge(opaque)]
pub struct YapHost;

#[bridge]
impl YapHost {
    pub fn initialize() -> Result<(), Error> {
        // The prototype deliberately keeps this runtime for the process lifetime.
        // Binding generation does not initialize it.
        static RUNTIME: std::sync::LazyLock<std::io::Result<tokio::runtime::Runtime>> =
            std::sync::LazyLock::new(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
            });
        let runtime = RUNTIME.as_ref().map_err(|e| Error::new(e.to_string()))?;
        if let Some(path) = std::env::var_os("YAP_DATA_DIR") {
            opfs::persistent::configure_root(path)?;
        } else {
            // Preserve the existing macOS Application Support/Yap location.
            opfs::persistent::configure_app("", "", "Yap")?;
        }
        bridgerton::native::set_tokio_handle(runtime.handle().clone())
    }
}

#[cfg(test)]
mod tests {
    use bridgerton::value::{decode, encode};

    #[test]
    fn smoke_fixtures_use_the_real_event_serialization() {
        use weapon::data_model::{Event, EventType, Timestamped};
        for json in [
            include_str!("../smoke/event-0.json"),
            include_str!("../smoke/event-1.json"),
        ] {
            let event: Timestamped<EventType<<yap_frontend_rs::DeckEvent as Event>::Versioned>> =
                serde_json::from_str(json).unwrap();
            assert_eq!(
                serde_json::to_value(event).unwrap(),
                serde_json::from_str::<serde_json::Value>(json).unwrap()
            );
        }
    }

    #[test]
    fn native_transport_preserves_current_event_json_and_legacy_aliases() {
        // These are existing storage spellings, independent of the native codec.
        for content in [
            serde_json::json!({"type":"SetDailyReviewTarget", "daily_review_target":"Intense"}),
            serde_json::json!({"type":"TranslationChallenge", "review":{
                "type":"Graded", "challenge":"bonjour", "submission":"salut",
                "literals":[], "phrases":[["語", true], ["a", null]]
            }}),
        ] {
            let json = serde_json::json!({"type":"Language", "target_language":"French",
                "native_language":"English", "content":content});
            let original: yap_frontend_rs::DeckEvent =
                serde_json::from_value(json.clone()).unwrap();
            let copied: yap_frontend_rs::DeckEvent = decode(&encode(&original).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(copied).unwrap(), json);
        }
        let legacy = serde_json::json!({"type":"SetGoal", "goal":{"type":"PimsleurLesson", "level":1,"unit":2}});
        let event: yap_frontend_rs::LanguageEventContent = serde_json::from_value(legacy).unwrap();
        let copied: yap_frontend_rs::LanguageEventContent =
            decode(&encode(&event).unwrap()).unwrap();
        assert_eq!(copied, event);
        assert_eq!(
            serde_json::to_value(copied).unwrap(),
            serde_json::json!({
                "type":"SetSentenceList", "sentence_list":{"type":"PimsleurLesson", "level":1,"lesson":2}
            })
        );
    }

    #[test]
    fn generic_sync_state_discovers_and_round_trips_its_dependencies() {
        use std::collections::BTreeMap;
        use weapon::data_model::SyncState;
        let timestamp = chrono::DateTime::from_timestamp(-1, 123_456_789).unwrap();
        let state = SyncState {
            remote_clock: BTreeMap::from([(
                "reviews".to_owned(),
                BTreeMap::from([("device".to_owned(), usize::MAX)]),
            )]),
            last_sync_started: Some(timestamp),
            last_sync_finished: None,
            last_sync_error: Some("offline 語".to_owned()),
        };
        assert_eq!(
            decode::<SyncState<String, String>>(&encode(&state).unwrap()).unwrap(),
            state
        );
        let definition = bridgerton::exports::definition().unwrap();
        let swift = definition.types.swift();
        assert!(swift.contains("struct `SyncState_String_String`"));
        assert!(swift.contains("Dictionary<String, Dictionary<String, UInt64>>"));
        assert_eq!(
            swift
                .matches("public struct `SyncState_String_String`")
                .count(),
            1
        );
    }
}
