//! # EventStreamStore
//! Weapon allows multiple "event streams" to be created. Each event stream combines events from all of a user's devices.
//! For example, in a Google Docs-like app, you could have one event stream for each document.
//! (This allows the memory consumption to be constant w.r.t. the number of documents, as only the events for the currently-active document would need to be loaded. Although the active document could still have a lot of events and use a lot of memory that way.)

use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;

use crate::data_model::{EventType, Timestamped};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EventStreamStore<Device: Eq + Clone + Hash, Event: Ord + Clone> {
    events: HashMap<Device, BTreeSet<Event>>,
}

impl<Device: Eq + Clone + Hash, Event: Ord + Clone> EventStreamStore<Device, Event> {
    pub fn events(&self) -> &HashMap<Device, BTreeSet<Event>> {
        &self.events
    }
}

impl<Device: Eq + Hash + Clone, Event: Ord + Clone> Default for EventStreamStore<Device, Event> {
    fn default() -> Self {
        Self {
            events: HashMap::new(),
        }
    }
}

impl<Device: Eq + Hash + Clone, Event: Ord + Clone> EventStreamStore<Device, Timestamped<Event>> {
    pub fn len_device(&self, device: &Device) -> usize {
        self.events.get(device).map(|set| set.len()).unwrap_or(0)
    }

    pub(crate) fn valid_to_add_events<A>(
        &self,
        key: &Device,
        mut events: Vec<Timestamped<A>>,
    ) -> Option<ValidToAddEvents<Timestamped<A>>> {
        // Early return if events is empty
        if events.is_empty() {
            return None;
        }

        events.sort_by_key(|event| event.within_device_events_index);

        // Check that there are no gaps in the events
        // Note: this is safe because we know that events is not empty
        for i in 0..events.len() - 1 {
            if events[i].within_device_events_index + 1 != events[i + 1].within_device_events_index
            {
                log::warn!("Gap detected in events");
                return None;
            }
        }

        // Check that the lowest event has the index of the current length
        let expected_index = self.events().get(key).map(BTreeSet::len).unwrap_or(0);
        if events[0].within_device_events_index != expected_index {
            log::warn!(
                "Event out of order - expected index {}, got {}",
                expected_index,
                events[0].within_device_events_index
            );
            return None;
        }

        Some(ValidToAddEvents { events })
    }

    pub(crate) fn add_device_events(
        &mut self,
        key: Device,
        events: ValidToAddEvents<Timestamped<Event>>,
    ) -> usize {
        let mut events_added = 0;

        // double check the events are still valid
        let Some(events) = self.valid_to_add_events(&key, events.events) else {
            return events_added;
        };

        let stream = self.events.entry(key.clone()).or_default();

        for event in events.events {
            events_added += 1;
            stream.insert(event);
        }

        events_added
    }
}

impl<K: Eq + Hash + Clone, T: Ord + Clone> EventStreamStore<K, T> {
    /// Add an event without checking if it's out of order.
    #[allow(unused)]
    pub fn add_event_unchecked(&mut self, key: K, event: T) {
        self.events.entry(key).or_default().insert(event);
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        // Collect all iterators from the OrdSets
        let mut iters: Vec<_> = self
            .events
            .values()
            .map(|set| set.iter().peekable())
            .collect();

        // Use a custom iterator that performs a k-way merge
        std::iter::from_fn(move || {
            // Find the iterator with the smallest current element
            let mut min_idx = None;
            let mut min_val = None;

            for (idx, iter) in iters.iter_mut().enumerate() {
                if let Some(val) = iter.peek()
                    && (min_val.is_none() || val < min_val.unwrap())
                {
                    min_idx = Some(idx);
                    min_val = Some(val);
                }
            }

            // Advance the iterator that had the minimum value
            if let Some(idx) = min_idx {
                iters[idx].next()
            } else {
                None
            }
        })
    }

    pub fn num_events(&self) -> usize {
        self.events.values().flatten().count()
    }

    #[allow(unused)]
    pub fn map<U: Ord + Clone>(self, f: impl Fn(T) -> U + Clone) -> EventStreamStore<K, U> {
        EventStreamStore {
            events: self
                .events
                .into_iter()
                .map(|(k, vs)| (k, vs.into_iter().map(f.clone()).collect::<BTreeSet<U>>()))
                .collect(),
        }
    }
}

/// Compute application state from a store that holds versioned events.
impl<Device: Eq + Hash + Clone, VersionedEvent: Ord + Clone>
    EventStreamStore<Device, Timestamped<EventType<VersionedEvent>>>
{
    /// Compute application state by applying stored events.
    /// The `CurrentEvent` type is what events are converted to for processing.
    /// The store holds `CurrentEvent::Versioned` (which should equal `VersionedEvent`).
    pub fn state<CurrentEvent, A>(
        &self,
        initial_state: A::Partial,
        context: &CurrentEvent::Context,
    ) -> A
    where
        CurrentEvent: crate::Event<Versioned = VersionedEvent>,
        A: crate::AppState<Event = CurrentEvent>,
    {
        apply_events_and_metaevents::<CurrentEvent, A>(self.iter(), initial_state, context)
    }
}

/// Apply versioned events by converting them to current form during processing.
/// Events that return None from from_versioned are skipped (e.g., no-op migrations).
/// Context is passed to from_versioned for conversions that need external data.
pub(crate) fn apply_events_and_metaevents<'a, E, A>(
    events: impl Iterator<Item = &'a Timestamped<EventType<E::Versioned>>>,
    initial_state: A::Partial,
    context: &E::Context,
) -> A
where
    E: crate::data_model::Event + 'a,
    E::Versioned: 'a,
    A: crate::AppState<Event = E>,
{
    let mut state = initial_state;

    // Process events one at a time, passing context to from_versioned
    for event in events {
        match event {
            Timestamped {
                event: EventType::User(versioned_event),
                timestamp,
                within_device_events_index,
                timezone,
            } => {
                // Convert from versioned, passing context
                if let Some(current_event) = E::from_versioned(versioned_event, context) {
                    let timestamped = Timestamped {
                        event: current_event,
                        timestamp: *timestamp,
                        within_device_events_index: *within_device_events_index,
                        timezone: *timezone,
                    };
                    state = A::process_event(state, context, &timestamped);
                }
            }
            _ => unimplemented!(), // todo: remove?
        }
    }

    A::finalize(state, context)
}

pub struct ValidToAddEvents<Event> {
    events: Vec<Event>,
}

impl<Event> ValidToAddEvents<Timestamped<Event>> {
    pub(crate) fn try_map<A, Error, F: Fn(Event) -> Result<A, Error>>(
        self,
        f: F,
    ) -> Result<ValidToAddEvents<Timestamped<A>>, Error> {
        Ok(ValidToAddEvents {
            events: self
                .events
                .into_iter()
                .map(move |timestamped| timestamped.map(&f))
                .map(Timestamped::transpose)
                .collect::<Result<Vec<_>, Error>>()?,
        })
    }
}
