//! # Timestamped
//! Events created by a single device must be sequential. In other words, a device should never "forget" about an event.
//! To guarantee this, we store the `within_device_events_index` of each event. This is a monotonically increasing number that is unique within a device.
//! The nth event created by a device has a `within_device_events_index` of n.
//!
//! Events must also be able to be put in order across devices. This is enabled via the `timestamp` field.
//!
//! Alongside the UTC `timestamp`, we record the user's `timezone` (offset from UTC) at the moment
//! the event was created. This lets us reason about local time-of-day, streaks, and behavior
//! patterns without losing information to UTC normalization. Events serialized before this field
//! existed deserialize to `None`; consumers fall back to the deck's current timezone for those,
//! preserving the pre-timezone bucketing behavior for already-persisted history.

/// (De)serialize an `Option<chrono::FixedOffset>` as its offset-from-UTC in seconds (`i32`).
///
/// `FixedOffset` has no `Serialize`/`Deserialize`/`Ord` impls we can rely on here, so we store
/// the raw `local_minus_utc()` seconds. This is stable on disk and trivially orderable. `None`
/// (an event recorded before timezones existed) round-trips as a missing/null field.
mod timezone_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        tz: &Option<chrono::FixedOffset>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        tz.map(|tz| tz.local_minus_utc()).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<chrono::FixedOffset>, D::Error> {
        let Some(seconds) = Option::<i32>::deserialize(deserializer)? else {
            return Ok(None);
        };
        chrono::FixedOffset::east_opt(seconds)
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("invalid timezone offset"))
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Timestamped<E> {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub within_device_events_index: usize,
    /// The user's timezone (offset from UTC, in seconds) when the event was created. `None` for
    /// events recorded before this field existed — consumers fall back to the deck's current
    /// timezone for those.
    #[serde(default, with = "timezone_serde")]
    pub timezone: Option<chrono::FixedOffset>,
    pub event: E,
}

/// Ordering ignores `timezone`: events are ordered by `(timestamp, within_device_events_index,
/// event)`, matching the pre-timezone behavior. `FixedOffset` carries no ordering meaning here
/// and including it would only break ties between otherwise-identical events.
impl<E: Ord> Ord for Timestamped<E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            &self.timestamp,
            &self.within_device_events_index,
            &self.event,
        )
            .cmp(&(
                &other.timestamp,
                &other.within_device_events_index,
                &other.event,
            ))
    }
}

impl<E: Ord> PartialOrd for Timestamped<E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<E> Timestamped<E> {
    pub fn map<G, F: Fn(E) -> G>(self, f: F) -> Timestamped<G> {
        Timestamped {
            timestamp: self.timestamp,
            within_device_events_index: self.within_device_events_index,
            timezone: self.timezone,
            event: f(self.event),
        }
    }

    pub fn as_ref(&self) -> Timestamped<&E> {
        Timestamped {
            timestamp: self.timestamp,
            within_device_events_index: self.within_device_events_index,
            timezone: self.timezone,
            event: &self.event,
        }
    }

    pub fn map_ref<G, F: Fn(&E) -> G>(&self, f: F) -> Timestamped<G> {
        Timestamped {
            timestamp: self.timestamp,
            within_device_events_index: self.within_device_events_index,
            timezone: self.timezone,
            event: f(&self.event),
        }
    }
}

impl<E, Error> Timestamped<Result<E, Error>> {
    pub fn transpose(self) -> Result<Timestamped<E>, Error> {
        let Timestamped {
            event,
            timestamp,
            within_device_events_index,
            timezone,
        } = self;
        event.map(|event| Timestamped {
            event,
            timestamp,
            within_device_events_index,
            timezone,
        })
    }
}

impl<E: crate::Event> crate::Event for Timestamped<E> {
    type Versioned = Timestamped<E::Versioned>;
    type Context = E::Context;

    fn to_versioned(&self) -> Self::Versioned {
        self.map_ref(|e| e.to_versioned())
    }

    fn from_versioned(versioned: &Self::Versioned, context: &Self::Context) -> Option<Self> {
        E::from_versioned(&versioned.event, context).map(|event| Timestamped {
            timestamp: versioned.timestamp,
            within_device_events_index: versioned.within_device_events_index,
            timezone: versioned.timezone,
            event,
        })
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
