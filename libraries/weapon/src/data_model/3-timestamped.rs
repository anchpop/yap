//! # Timestamped
//! Events created by a single device must be sequential. In other words, a device should never "forget" about an event.
//! To guarantee this, we store the `within_device_events_index` of each event. This is a monotonically increasing number that is unique within a device.
//! The nth event created by a device has a `within_device_events_index` of n.
//!
//! Events must also be able to be put in order across devices. This is enabled via the `timestamp` field.

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi, into_wasm_abi))]
pub struct Timestamped<E> {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub within_device_events_index: usize,
    pub event: E,
}

impl<E> Timestamped<E> {
    pub fn map<G, F: Fn(E) -> G>(self, f: F) -> Timestamped<G> {
        Timestamped {
            timestamp: self.timestamp,
            within_device_events_index: self.within_device_events_index,
            event: f(self.event),
        }
    }

    pub fn as_ref(&self) -> Timestamped<&E> {
        Timestamped {
            timestamp: self.timestamp,
            within_device_events_index: self.within_device_events_index,
            event: &self.event,
        }
    }
}

impl<E, Error> Timestamped<Result<E, Error>> {
    pub fn transpose(self) -> Result<Timestamped<E>, Error> {
        let Timestamped {
            event,
            timestamp,
            within_device_events_index,
        } = self;
        event.map(|event| Timestamped {
            event,
            timestamp,
            within_device_events_index,
        })
    }
}

impl<E: crate::Event> crate::Event for Timestamped<E> {
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        let s = self.as_ref().map(|e| e.to_json()).transpose()?;
        serde_json::to_value(&s)
    }

    fn from_json(json: &serde_json::Value) -> Result<Self, serde_json::Error> {
        let s = serde_json::from_value::<Timestamped<serde_json::Value>>(json.clone())?;
        s.map(|e| E::from_json(&e)).transpose()
    }
}

pub trait IndexedEvent {
    fn within_device_events_index(&self) -> usize;
}

impl<E> IndexedEvent for Timestamped<E> {
    fn within_device_events_index(&self) -> usize {
        self.within_device_events_index
    }
}
