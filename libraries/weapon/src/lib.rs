#![doc = include_str!("../readme.md")]
#[cfg(feature = "supabase")]
pub mod supabase;

#[cfg(feature = "opfs")]
pub mod opfs;

pub mod data_model;

use crate::data_model::{Event, Timestamped};

/// Trait for computing application state from events.
pub trait AppState: Sized {
    type Event: Event;

    /// The partial state type built up during event processing.
    type Partial;

    /// Process an event, updating the state.
    /// This is called for each event when applying multiple events.
    /// Context is provided for event processing that needs external data.
    fn process_event(
        state: Self::Partial,
        context: &<Self::Event as Event>::Context,
        event: &Timestamped<Self::Event>,
    ) -> Self::Partial;

    /// Finalize the state by computing any derived state (e.g., statistical models).
    /// This is called once after all events have been processed.
    /// Context is provided for finalization that needs external data.
    fn finalize(state: Self::Partial, context: &<Self::Event as Event>::Context) -> Self;
}
