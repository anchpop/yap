use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::Arc;

use crate::data_model::{
    DirtyState, DirtyTracker, EventStreamStore, EventType, ListenerKey, StreamStore, Timestamped,
};

use super::DirtyOnDerefMut;

pub struct EventStore<Stream: Eq + Hash + Clone, Device: Eq + Hash + Clone> {
    streams: HashMap<Stream, DirtyTracker<Box<dyn StreamStore<Device>>>>,
    listeners: slotmap::SlotMap<slotmap::DefaultKey, Arc<dyn Fn(ListenerKey, Stream)>>,

    /// Updated whenever a sync target is updated.
    sync_states: SyncStates<Stream, Device>,
}

impl<Stream: Eq + Hash + Clone, Device: Eq + Hash + Clone> Default for EventStore<Stream, Device> {
    fn default() -> Self {
        Self {
            streams: HashMap::new(),
            listeners: Default::default(),

            sync_states: Default::default(),
        }
    }
}

impl<Stream: Eq + Hash + Clone + 'static, Device: Eq + Hash + Clone + 'static>
    EventStore<Stream, Device>
{
    pub fn drain_due_notifications(&mut self) -> Vec<Box<dyn FnOnce()>> {
        let mut notifications: Vec<Box<dyn FnOnce()>> = Vec::new();
        for (stream_id, event_stream) in self.streams.iter_mut() {
            let exclude_key = match &event_stream.dirty_state {
                DirtyState::Clean => continue,
                DirtyState::DirtyExcept(key) => Some(*key),
                DirtyState::DirtyAll => None,
            };

            // Reset to clean after draining
            event_stream.dirty_state = DirtyState::Clean;

            for (key, listener) in self.listeners.iter() {
                let listener_key = ListenerKey(key);
                if exclude_key == Some(listener_key) {
                    continue;
                }
                let listener = listener.clone();
                let stream_id = stream_id.clone();
                notifications.push(Box::new(move || listener(listener_key, stream_id)));
            }
        }
        notifications
    }
}

impl<Stream: Eq + Hash + Clone + Ord, Device: Eq + Hash + Clone + Ord + 'static>
    EventStore<Stream, Device>
{
    pub fn iter(&self) -> impl Iterator<Item = (&Stream, &dyn StreamStore<Device>)> {
        self.streams
            .iter()
            .map(|(stream, event_stream)| (stream, event_stream.store().as_ref()))
    }

    pub fn vector_clock(&self) -> Clock<Stream, Device> {
        self.iter()
            .map(|(stream, event_stream_store)| {
                (
                    stream.clone(),
                    event_stream_store
                        .num_events_per_device()
                        .into_iter()
                        .map(|(device, count)| (device.clone(), count))
                        .collect(),
                )
            })
            .collect()
    }
}

impl<Stream: Eq + Hash + Clone + Ord, Device: Eq + Hash + Clone + Ord + 'static>
    EventStore<Stream, Device>
{
    pub fn get_raw(&self, stream: Stream) -> Option<&dyn StreamStore<Device>> {
        self.streams.get(&stream).map(|s| s.store().as_ref())
    }

    pub fn get<Event: Ord + Clone + crate::Event + 'static>(
        &self,
        stream: Stream,
    ) -> Option<&EventStreamStore<Device, Timestamped<Event>>> {
        self.get_raw(stream).map(|s| {
            let s: &dyn Any = s;
            s
                .downcast_ref::<EventStreamStore<Device, Timestamped<Event>>>()
                .unwrap_or_else(||
                    panic!(
                        "Type mismatch: expected an EventStreamStore<Device, Timestamped<Event>>, but got one that was different somehow from the expectation. Note: Event = {:?}", 
                        std::any::type_name::<Event>()
                    )
                )
        })
    }

    pub fn get_mut_raw(
        &mut self,
        stream: &Stream,
        modifier: Option<ListenerKey>,
    ) -> Option<DirtyOnDerefMut<'_, Box<dyn StreamStore<Device>>>> {
        let stream = self.streams.get_mut(stream);
        stream.map(|s| s.store_mut(modifier))
    }

    pub fn get_mut<Event: Ord + Clone + crate::Event + 'static>(
        &mut self,
        stream: &Stream,
        modifier: Option<ListenerKey>,
    ) -> Option<DirtyOnDerefMut<'_, EventStreamStore<Device, Timestamped<Event>>>> {
        self.get_mut_raw(stream, modifier).map(|s| {
            s.map(|s| {
                let s: &mut dyn Any = s.as_mut();
                s
                    .downcast_mut::<EventStreamStore<Device, Timestamped<Event>>>()
                    .unwrap_or_else(||
                        panic!(
                            "Type mismatch: expected an EventStreamStore<Device, Timestamped<Event>>, but got one that was different somehow from the expectation. Note: Event = {:?}", 
                            std::any::type_name::<Event>()
                        )
                    )
            })
        })
    }

    pub fn get_or_insert_default<Event: Ord + Clone + crate::Event + 'static>(
        &mut self,
        stream: Stream,
        modifier: Option<ListenerKey>,
    ) -> DirtyOnDerefMut<'_, EventStreamStore<Device, Timestamped<Event>>> {
        if !self.streams.contains_key(&stream) {
            let store = DirtyTracker::<EventStreamStore<Device, Timestamped<Event>>>::default();
            let store = store.map(|s| Box::new(s) as Box<dyn StreamStore<Device>>);
            self.streams.insert(stream.clone(), store);
        }
        self.get_mut::<Event>(&stream, modifier)
            .expect("stream must exist at this point")
    }

    /// The listener is invoked whenever a new stream is added.
    pub fn register_listener(
        &mut self,
        listener: impl Fn(ListenerKey, Stream) + 'static,
    ) -> ListenerKey {
        let key = self.listeners.insert(Arc::new(listener));
        ListenerKey(key)
    }

    /// Unregister a previously registered store-level listener.
    pub fn unregister_listener(&mut self, token: ListenerKey) {
        self.listeners.remove(token.0);
    }

    pub fn sync_state(&self, target: SyncTarget) -> Option<&SyncState<Stream, Device>> {
        self.sync_states.get(&target)
    }

    pub fn loaded_at_least_once(&self, stream: &Stream) -> bool {
        self.streams
            .get(stream)
            .map(|s| s.loaded_at_least_once())
            .unwrap_or(false)
    }

    /// returns true if the `loaded` marker was changed
    pub fn mark_loaded(&mut self, stream: Stream, modifier: Option<ListenerKey>) -> bool {
        let Some(stream) = self.streams.get_mut(&stream) else {
            return false;
        };

        stream.mark_loaded(modifier)
    }
}

impl<Stream: Eq + Hash + Clone + Ord, Device: Eq + Hash + Clone + Ord + 'static>
    EventStore<Stream, Device>
{
    pub fn add_events<Event, EventsIter>(
        &mut self,
        stream: Stream,
        events: EventsIter,
        modifier: Option<ListenerKey>,
    ) -> usize
    where
        Event: Ord + Clone + crate::Event + 'static,
        EventsIter: IntoIterator<Item = (Device, Vec<Timestamped<Event>>)>,
    {
        let mut events_added = 0;
        for (device, events) in events {
            events_added += self.add_device_events(stream.clone(), device, events, modifier);
        }
        events_added
    }

    pub fn add_device_events<Event>(
        &mut self,
        stream: Stream,
        device: Device,
        events: Vec<Timestamped<Event>>,
        modifier: Option<ListenerKey>,
    ) -> usize
    where
        Event: Ord + Clone + crate::Event + 'static,
    {
        let store = self.get_or_insert_default(stream, modifier);

        let Some(valid_to_add) = store.valid_to_add_events(&device, events) else {
            return 0;
        };

        let mut store = store;

        store.add_device_events(device, valid_to_add)
    }

    pub fn add_device_events_jsons(
        &mut self,
        stream: Stream,
        device: Device,
        events: Vec<Timestamped<serde_json::Value>>,
        modifier: Option<ListenerKey>,
    ) -> usize {
        let Some(store) = self.get_mut_raw(&stream, modifier) else {
            log::error!("Cannot insert events for stream as it does not exist");
            return 0;
        };

        let Some(valid_to_add) = store.valid_to_add_event_jsons(&device, events) else {
            return 0;
        };

        let mut store = store;

        store
            .add_device_event_jsons(device, valid_to_add)
            .inspect_err(|e| {
                log::error!("Error deserializing event JSON into event type: {e:?}");
            })
            .unwrap_or(0)
    }

    pub fn add_device_event<Event>(
        &mut self,
        stream: Stream,
        device: Device,
        event: Timestamped<Event>,
        modifier: Option<ListenerKey>,
    ) -> usize
    where
        Event: Ord + Clone + crate::Event + 'static,
    {
        self.add_device_events(stream, device, vec![event], modifier)
    }
}

impl<Stream: Eq + Hash + Clone + Ord, Device: Eq + Hash + Clone + Ord + 'static>
    EventStore<Stream, Device>
{
    pub fn add_raw_event<Event>(
        &mut self,
        stream: Stream,
        device: Device,
        event: Event,
        modifier: Option<ListenerKey>,
    ) where
        Event: Ord + Clone + crate::Event + 'static,
    {
        let event = Timestamped {
            event: EventType::User(event),
            timestamp: chrono::Utc::now(),
            within_device_events_index: self
                .get_or_insert_default::<EventType<Event>>(stream.clone(), modifier)
                .len_device(&device),
        };

        self.add_device_event(stream, device, event, modifier);
    }

    /// Returns None if there are no unsynced events
    pub fn get_timestamp_of_earliest_unsynced_event(
        &self,
        target: SyncTarget,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        let remote_clock = self
            .sync_states
            .get(&target)
            .map(|s| s.remote_clock.clone())
            .unwrap_or_default();
        let mut earliest: Option<chrono::DateTime<chrono::Utc>> = None;

        for (stream_id, event_stream) in &self.streams {
            let device_sync_map = remote_clock.get(stream_id).cloned().unwrap_or_default();

            let candidate = event_stream
                .store()
                .timestamp_of_earliest_unsynced_event(&device_sync_map);

            earliest = match (earliest, candidate) {
                (None, Some(candidate)) => Some(candidate),
                (Some(current), Some(candidate)) => Some(current.min(candidate)),
                (Some(current), None) => Some(current),
                (None, None) => None,
            };
        }

        earliest
    }
}

pub type Clock<Stream, Device> = BTreeMap<Stream, BTreeMap<Device, usize>>;
pub type SyncStates<Stream, Device> = HashMap<SyncTarget, SyncState<Stream, Device>>;

fn join_clocks<Stream, Device>(
    clock1: Clock<Stream, Device>,
    clock2: Clock<Stream, Device>,
) -> Clock<Stream, Device>
where
    Device: Eq + Hash + Clone + Ord,
    Stream: Eq + Hash + Clone + Ord,
{
    // Merge the two clocks by taking the element-wise max for each
    // (stream, device) pair across both clocks.
    let mut result = clock1;

    for (stream, device_map) in clock2 {
        let entry = result.entry(stream).or_default();
        for (device, count) in device_map {
            entry
                .entry(device)
                .and_modify(|c| {
                    if *c < count {
                        *c = count;
                    }
                })
                .or_insert(count);
        }
    }

    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub enum SyncTarget {
    Supabase,
    Opfs,
}

impl<Stream: Eq + Hash + Clone + Ord, Device: Eq + Hash + Clone + Ord> EventStore<Stream, Device> {
    /// Join and record the latest sync clock for a specific target.
    pub fn update_sync_clock(&mut self, target: SyncTarget, new_clock: Clock<Stream, Device>) {
        let state = self.sync_states.entry(target).or_default();
        state.remote_clock = join_clocks(state.remote_clock.clone(), new_clock);
    }

    pub fn mark_sync_started(&mut self, target: SyncTarget) {
        let state = self.sync_states.entry(target).or_default();
        state.last_sync_started = Some(chrono::Utc::now());
    }

    pub fn mark_sync_finished(&mut self, target: SyncTarget, error: Option<String>) {
        let state = self.sync_states.entry(target).or_default();
        state.last_sync_finished = Some(chrono::Utc::now());
        state.last_sync_error = error;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
#[serde(bound(
    serialize = "Stream: serde::Serialize + Eq + Hash + Ord, Device: serde::Serialize + Eq + Hash + Ord",
    deserialize = "Stream: serde::Deserialize<'de> + Eq + Hash + Ord, Device: serde::Deserialize<'de> + Eq + Hash + Ord"
))]
pub struct SyncState<Stream, Device> {
    pub remote_clock: Clock<Stream, Device>,

    /// If last_sync_started > last_sync_finished, then the sync is in progress.
    pub last_sync_started: Option<chrono::DateTime<chrono::Utc>>,
    pub last_sync_finished: Option<chrono::DateTime<chrono::Utc>>,

    /// If last_sync_error is Some, then the last sync failed. Gets reset to None when the next sync succeeds.
    pub last_sync_error: Option<String>,
}

impl<Stream, Device> Default for SyncState<Stream, Device> {
    fn default() -> Self {
        Self {
            remote_clock: BTreeMap::new(),
            last_sync_started: None,
            last_sync_finished: None,
            last_sync_error: None,
        }
    }
}
