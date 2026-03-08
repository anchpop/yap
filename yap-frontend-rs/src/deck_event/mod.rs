//! Deck event types for tracking user actions and state changes.

pub mod current;
pub mod v1;
pub mod v2;

use crate::Context;
use serde::{Deserialize, Serialize};
use weapon::data_model::Event;

// Re-export current types for convenience
pub use current::*;

#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "version")]
#[allow(private_interfaces)] // old types are not public outside of this module to prevent them from being used in our code
pub enum VersionedDeckEvent {
    V1(v1::DeckEvent),
    V2(v2::DeckEvent),
    V3(current::DeckEvent),
}

impl Event for current::DeckEvent {
    type Versioned = VersionedDeckEvent;
    type Context = Context;

    fn to_versioned(&self) -> Self::Versioned {
        VersionedDeckEvent::from(self.clone())
    }

    fn from_versioned(versioned: &Self::Versioned, context: &Self::Context) -> Option<Self> {
        match versioned {
            VersionedDeckEvent::V1(event) => event.clone().into_v2()?.into_v3(context),
            VersionedDeckEvent::V2(event) => event.clone().into_v3(context),
            VersionedDeckEvent::V3(event) => Some(event.clone()),
        }
    }
}

impl From<current::DeckEvent> for VersionedDeckEvent {
    fn from(event: current::DeckEvent) -> Self {
        VersionedDeckEvent::V3(event)
    }
}
