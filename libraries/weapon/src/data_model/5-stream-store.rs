//! A trait for EventStreamStores, that allows type erasure

use std::{
    any::Any,
    collections::{BTreeMap, HashMap},
};

use crate::data_model::{EventStreamStore, Timestamped, ValidToAddEvents};
use std::hash::Hash;

pub trait StreamStore<Device>: Any {
    fn num_events_per_device(&self) -> HashMap<&Device, usize>;

    fn num_events(&self) -> usize {
        self.num_events_per_device().values().sum()
    }

    fn jsons(&self, device: &Device, skip: usize) -> Vec<Timestamped<serde_json::Value>>;

    fn valid_to_add_event_jsons(
        &self,
        device: &Device,
        events: Vec<Timestamped<serde_json::Value>>,
    ) -> Option<ValidToAddEvents<Timestamped<serde_json::Value>>>;

    fn add_device_event_jsons(
        &mut self,
        device: Device,
        events: ValidToAddEvents<Timestamped<serde_json::Value>>,
    ) -> Result<usize, serde_json::Error>;

    fn timestamp_of_earliest_unsynced_event(
        &self,
        sync_state: &BTreeMap<Device, usize>,
    ) -> Option<chrono::DateTime<chrono::Utc>>;
}

impl<Device: Ord + Eq + Clone + Hash + 'static, Event: crate::Event + 'static> StreamStore<Device>
    for EventStreamStore<Device, Timestamped<Event>>
{
    fn num_events_per_device(&self) -> HashMap<&Device, usize> {
        self.events()
            .iter()
            .map(|(device, events)| (device, events.len()))
            .collect::<HashMap<&Device, usize>>()
    }

    fn jsons(&self, device: &Device, skip: usize) -> Vec<Timestamped<serde_json::Value>> {
        self.events()
            .get(device)
            .map(|events| {
                events
                    .iter()
                    .skip(skip)
                    .map(|event| event.as_ref().map(|event| event.to_json().unwrap()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn valid_to_add_event_jsons(
        &self,
        device: &Device,
        events: Vec<Timestamped<serde_json::Value>>,
    ) -> Option<ValidToAddEvents<Timestamped<serde_json::Value>>> {
        self.valid_to_add_events(device, events)
    }

    fn add_device_event_jsons(
        &mut self,
        device: Device,
        events: ValidToAddEvents<Timestamped<serde_json::Value>>,
    ) -> Result<usize, serde_json::Error> {
        let events = events.try_map(|event| Event::from_json(&event))?;
        Ok(self.add_device_events(device, events))
    }

    fn timestamp_of_earliest_unsynced_event(
        &self,
        sync_state: &BTreeMap<Device, usize>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        let mut earliest: Option<_> = None;
        for (device_id, events_set) in self.events() {
            let synced_count = sync_state.get(device_id).copied().unwrap_or(0);

            if events_set.len() > synced_count {
                if let Some(ev) = events_set
                    .iter()
                    .find(|e| e.within_device_events_index == synced_count)
                {
                    let candidate = ev.timestamp;
                    earliest = match earliest {
                        None => Some(candidate),
                        Some(current) => Some(current.min(candidate)),
                    };
                }
            }
        }
        earliest
    }
}
