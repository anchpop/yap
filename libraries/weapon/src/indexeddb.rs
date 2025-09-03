use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use js_sys;
use web_sys::BroadcastChannel;

use idb::{
    Database, DatabaseEvent, Error, Factory, IndexParams, KeyPath, ObjectStoreParams,
    TransactionMode,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

use crate::data_model::{Clock, EventStore, EventType, ListenerKey, SyncTarget, Timestamped};

const DB_NAME: &str = "weapon_events";
const DB_VERSION: u32 = 1;
const STORE_NAME: &str = "events";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventRecord {
    user_id: String,
    stream_id: String,
    device_id: String,
    event_index: usize,
    event: serde_json::Value,
}

#[derive(Debug)]
pub struct EventDatabase {
    database: Database,
    user_id: String,
}

impl Clone for EventDatabase {
    fn clone(&self) -> Self {
        panic!("EventDatabase cannot be cloned - use Arc<EventDatabase> instead")
    }
}

impl EventDatabase {
    pub async fn new(user_id: &str) -> Result<Self, Error> {
        let factory = Factory::new()?;
        let mut open_request = factory.open(DB_NAME, Some(DB_VERSION))?;

        open_request.on_upgrade_needed(|event| {
            let database = event.database().unwrap();

            // Create object store with auto-incrementing primary key
            let mut store_params = ObjectStoreParams::new();
            store_params.auto_increment(true);
            store_params.key_path(Some(KeyPath::new_single("id")));

            let store = database
                .create_object_store(STORE_NAME, store_params)
                .unwrap();

            // Create compound index for user_id + stream_id + device_id + event_index
            let mut index_params = IndexParams::new();
            index_params.unique(true);

            store
                .create_index(
                    "user_stream_device_index",
                    KeyPath::new_array(["user_id", "stream_id", "device_id", "event_index"]),
                    Some(index_params.clone()),
                )
                .unwrap();

            // Create index for user_id + stream_id
            let mut index_params = IndexParams::new();
            index_params.unique(false);

            store
                .create_index(
                    "user_stream",
                    KeyPath::new_array(["user_id", "stream_id"]),
                    Some(index_params.clone()),
                )
                .unwrap();

            // Create index for user_id + stream_id + device_id
            store
                .create_index(
                    "user_stream_device",
                    KeyPath::new_array(["user_id", "stream_id", "device_id"]),
                    Some(index_params),
                )
                .unwrap();
        });

        let database = open_request.await?;

        Ok(Self {
            database,
            user_id: user_id.to_string(),
        })
    }

    async fn add_event<Event: crate::Event>(
        &self,
        stream_id: &str,
        device_id: &str,
        event: &Timestamped<EventType<Event>>,
    ) -> Result<JsValue, Error>
    where
        Event::Versioned: Serialize + for<'de> Deserialize<'de>,
    {
        let transaction = self
            .database
            .transaction(&[STORE_NAME], TransactionMode::ReadWrite)?;
        let store = transaction.object_store(STORE_NAME)?;

        let versioned_event = event.clone().map(|e| e.version());
        let event_json = serde_json::to_value(&versioned_event).unwrap();

        let record = EventRecord {
            user_id: self.user_id.clone(),
            stream_id: stream_id.to_string(),
            device_id: device_id.to_string(),
            event_index: event.within_device_events_index,
            event: event_json,
        };

        let serialized = serde_wasm_bindgen::to_value(&record).unwrap();
        let id = store.add(&serialized, None)?.await?;

        transaction.commit()?.await?;

        Ok(id)
    }

    async fn get_all_stream_events<Event: crate::Event>(
        &self,
        stream_id: &str,
    ) -> Result<BTreeMap<String, Vec<Timestamped<EventType<Event>>>>, Error>
    where
        Event::Versioned: Serialize + for<'de> Deserialize<'de>,
    {
        let transaction = self
            .database
            .transaction(&[STORE_NAME], TransactionMode::ReadOnly)?;
        let store = transaction.object_store(STORE_NAME)?;
        let index = store.index("user_stream")?;

        // Query all events for this user and stream
        let cursor_request = index.open_cursor(None, None)?;
        let mut cursor = match cursor_request.await? {
            Some(c) => c.into_managed(),
            None => return Ok(BTreeMap::new()),
        };

        let mut device_events: BTreeMap<String, Vec<Timestamped<EventType<Event>>>> =
            BTreeMap::new();

        loop {
            if let Some(value) = cursor.value()? {
                let value_clone = value.clone();
                let record: EventRecord = serde_wasm_bindgen::from_value(value)
                    .map_err(|_| Error::UnexpectedJsType("EventRecord", value_clone))?;

                if record.user_id == self.user_id && record.stream_id == stream_id.to_string() {
                    let versioned_event: Timestamped<EventType<Event::Versioned>> =
                        serde_json::from_value(record.event).unwrap();
                    let unversioned_event = versioned_event.map(|e| e.deversion());
                    device_events
                        .entry(record.device_id)
                        .or_default()
                        .push(unversioned_event);
                }

                cursor.next(None).await?;
            } else {
                break;
            }
        }

        transaction.await?;

        Ok(device_events)
    }

    async fn get_clock(&self, only_stream: Option<&str>) -> Result<Clock<String, String>, Error> {
        let transaction = self
            .database
            .transaction(&[STORE_NAME], TransactionMode::ReadOnly)?;
        let store = transaction.object_store(STORE_NAME)?;

        let mut clock: Clock<String, String> = BTreeMap::new();

        if let Some(stream_id) = only_stream {
            let index = store.index("user_stream")?;

            let cursor_request = index.open_cursor(None, None)?;
            let mut cursor = match cursor_request.await? {
                Some(c) => c.into_managed(),
                None => return Ok(BTreeMap::new()),
            };

            let mut device_counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut device_indices: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();

            loop {
                if let Some(value) = cursor.value()? {
                    let value_clone = value.clone();
                    let record: serde_json::Value = serde_wasm_bindgen::from_value(value)
                        .map_err(|_| Error::UnexpectedJsType("serde_json::Value", value_clone))?;

                    if let (
                        Some(user_id),
                        Some(stream_id_val),
                        Some(device_id),
                        Some(event_index),
                    ) = (
                        record.get("user_id").and_then(|v| v.as_str()),
                        record.get("stream_id").and_then(|v| v.as_str()),
                        record.get("device_id").and_then(|v| v.as_str()),
                        record.get("event_index").and_then(|v| v.as_u64()),
                    ) {
                        if user_id == self.user_id && stream_id_val == stream_id {
                            device_indices
                                .entry(device_id.to_string())
                                .or_default()
                                .insert(event_index as usize);
                        }
                    }

                    cursor.next(None).await?;
                } else {
                    break;
                }
            }

            // Verify contiguity and set counts
            for (device_id, indices) in device_indices {
                for (expected, idx) in indices.iter().enumerate() {
                    if *idx != expected {
                        log::error!(
                            "IndexedDB index gap for stream {} device {}: expected {}, found {}",
                            stream_id,
                            device_id,
                            expected,
                            idx
                        );
                        panic!("IndexedDB device indices not contiguous");
                    }
                }
                device_counts.insert(device_id, indices.len());
            }

            clock.insert(stream_id.to_string(), device_counts);
        } else {
            // Get all streams for this user
            let cursor_request = store.open_cursor(None, None)?;
            let mut cursor = match cursor_request.await? {
                Some(c) => c.into_managed(),
                None => return Ok(BTreeMap::new()),
            };

            let mut stream_device_indices: BTreeMap<String, BTreeMap<String, BTreeSet<usize>>> =
                BTreeMap::new();

            loop {
                if let Some(value) = cursor.value()? {
                    let value_clone = value.clone();
                    let record: serde_json::Value = serde_wasm_bindgen::from_value(value)
                        .map_err(|_| Error::UnexpectedJsType("serde_json::Value", value_clone))?;

                    if let (Some(user_id), Some(stream_id), Some(device_id), Some(event_index)) = (
                        record.get("user_id").and_then(|v| v.as_str()),
                        record.get("stream_id").and_then(|v| v.as_str()),
                        record.get("device_id").and_then(|v| v.as_str()),
                        record.get("event_index").and_then(|v| v.as_u64()),
                    ) {
                        if user_id == self.user_id {
                            stream_device_indices
                                .entry(stream_id.to_string())
                                .or_default()
                                .entry(device_id.to_string())
                                .or_default()
                                .insert(event_index as usize);
                        }
                    }

                    cursor.next(None).await?;
                } else {
                    break;
                }
            }

            // Verify contiguity and build clock
            for (stream_id, device_indices) in stream_device_indices {
                let mut device_counts: BTreeMap<String, usize> = BTreeMap::new();

                for (device_id, indices) in device_indices {
                    for (expected, idx) in indices.iter().enumerate() {
                        if *idx != expected {
                            log::error!(
                                "IndexedDB index gap for stream {} device {}: expected {}, found {}",
                                stream_id,
                                device_id,
                                expected,
                                idx
                            );
                            panic!("IndexedDB device indices not contiguous");
                        }
                    }
                    device_counts.insert(device_id, indices.len());
                }

                clock.insert(stream_id, device_counts);
            }
        }

        transaction.await?;

        Ok(clock)
    }
}

impl<Event: Eq + Ord + Clone + crate::Event>
    EventStore<String, String, Timestamped<EventType<Event>>>
where
    Event::Versioned: serde::de::DeserializeOwned + serde::Serialize,
{
    pub async fn sync_with_indexeddb(
        store: &RefCell<EventStore<String, String, Timestamped<EventType<Event>>>>,
        database: &EventDatabase,
        stream_id_to_sync: Option<String>,
        modifier: Option<ListenerKey>,
    ) -> Result<(), Error> {
        store.borrow_mut().mark_sync_started(SyncTarget::Opfs);

        let result =
            Self::sync_with_indexeddb_inner(store, database, stream_id_to_sync.clone(), modifier)
                .await;

        match &result {
            Ok(()) => store
                .borrow_mut()
                .mark_sync_finished(SyncTarget::Opfs, None),
            Err(e) => store
                .borrow_mut()
                .mark_sync_finished(SyncTarget::Opfs, Some(format!("{e:?}"))),
        }

        result
    }

    async fn sync_with_indexeddb_inner(
        store: &RefCell<EventStore<String, String, Timestamped<EventType<Event>>>>,
        database: &EventDatabase,
        stream_id_to_sync: Option<String>,
        modifier: Option<ListenerKey>,
    ) -> Result<(), Error> {
        // 1) Load fresh events from IndexedDB into memory
        if let Some(stream_id) = stream_id_to_sync.clone() {
            Self::load_from_indexeddb(store, database, stream_id.clone(), modifier).await?;
        } else {
            // Get all stream IDs from the clock
            let clock = database.get_clock(None).await?;
            for (stream_id, _) in clock {
                Self::load_from_indexeddb(store, database, stream_id, modifier).await?;
            }
        }

        // 2) Save any in-memory events to IndexedDB
        if let Some(stream_id) = stream_id_to_sync.clone() {
            let _ = Self::save_to_indexeddb(store, database, stream_id.clone()).await?;
        } else {
            // Persist all streams present in the store
            let stream_ids: Vec<String> =
                store.borrow().iter().map(|(sid, _)| sid.clone()).collect();
            for stream_id in stream_ids {
                let _ = Self::save_to_indexeddb(store, database, stream_id.clone()).await?;
            }
        }

        // 3) Refresh IndexedDB clock and record it in sync state
        let final_clock = database.get_clock(stream_id_to_sync.as_deref()).await?;
        store
            .borrow_mut()
            .update_sync_clock(SyncTarget::Opfs, final_clock);

        Ok(())
    }

    async fn load_from_indexeddb(
        store: &RefCell<EventStore<String, String, Timestamped<EventType<Event>>>>,
        database: &EventDatabase,
        stream_id: String,
        modifier: Option<ListenerKey>,
    ) -> Result<(), Error> {
        // Get all device events for this stream
        let device_events = database.get_all_stream_events::<Event>(&stream_id).await?;

        for (device_id, events) in device_events {
            let current_num_events = store
                .borrow()
                .get(stream_id.clone())
                .map(|s| s.store.len_device(&device_id))
                .unwrap_or(0);

            // Filter to only fresh events (index >= current_num_events)
            let fresh_events: Vec<_> = events
                .into_iter()
                .filter(|e| e.within_device_events_index >= current_num_events)
                .collect();

            // Add fresh events to current state
            store
                .borrow_mut()
                .get_or_insert_default(stream_id.clone())
                .add_device_events(device_id, fresh_events, modifier);
        }

        Ok(())
    }

    async fn save_to_indexeddb(
        store: &RefCell<EventStore<String, String, Timestamped<EventType<Event>>>>,
        database: &EventDatabase,
        stream_id: String,
    ) -> Result<usize, Error> {
        let mut total_written: usize = 0;

        // Local desired counts per device for this stream
        let Some(device_events) = store.borrow().vector_clock().remove(&stream_id) else {
            log::warn!("Stream {stream_id} not found in store, skipping save");
            return Ok(0);
        };

        // Get on-disk clock for this stream
        let db_clock = database.get_clock(Some(&stream_id)).await?;
        let device_counts_in_db = db_clock.get(&stream_id).cloned().unwrap_or_default();

        for (device_id, _num_events) in device_events {
            let device_events_in_db = device_counts_in_db.get(&device_id).copied().unwrap_or(0);

            // Collect events with index >= device_events_in_db
            let events_to_write: Vec<Timestamped<EventType<Event>>> = {
                let store = store.borrow();
                let Some(stream) = store.get(stream_id.clone()) else {
                    log::error!(
                        "Stream {stream_id} not found in store, which should be impossible as we already checked for it"
                    );
                    continue;
                };
                let stream = &stream.store;
                let Some(device_events) = stream.events().get(&device_id.clone()) else {
                    log::error!(
                        "Device {device_id} not found in stream, which should be impossible as we already checked for it"
                    );
                    continue;
                };
                device_events
                    .iter()
                    .filter(|e| e.within_device_events_index >= device_events_in_db)
                    .cloned()
                    .collect::<Vec<_>>()
            };

            for event in events_to_write {
                database.add_event(&stream_id, &device_id, &event).await?;
                total_written += 1;
            }
        }

        // If we wrote anything, broadcast a message to other tabs
        #[cfg(target_arch = "wasm32")]
        if total_written > 0 {
            match BroadcastChannel::new("weapon-indexeddb-sync") {
                Ok(channel) => {
                    let obj = js_sys::Object::new();
                    js_sys::Reflect::set(&obj, &"type".into(), &"indexeddb-written".into())
                        .unwrap();
                    js_sys::Reflect::set(&obj, &"stream_id".into(), &stream_id.as_str().into())
                        .unwrap();

                    log::info!(
                        "Broadcasting indexeddb-written message for stream: {}",
                        stream_id
                    );
                    match channel.post_message(&obj) {
                        Ok(_) => log::info!("Message posted successfully"),
                        Err(e) => log::error!("Failed to post message: {:?}", e),
                    }
                }
                Err(e) => {
                    log::error!("Failed to create BroadcastChannel: {:?}", e);
                }
            }
        }

        Ok(total_written)
    }
}
