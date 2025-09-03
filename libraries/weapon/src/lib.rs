//! This is a library for enabling cross-device local-first event syncing.
//! It was created for Yap.Town, so it doesn't include much that was not needed for that project.
//!
//! Syncing strategy:
//! 1. Each of the user's devices gets a unique ID.
//! 2. As users use your app, instead of the app modifying the state directly, they generate "events". Events are associated with the device generated them, as well as a timestamp and an index within the device's events.
//! 3. Starting from a default initial state, these events are "applied" in chronological order to get the current state.
//! 4. When syncing:
//!   1. The user's device asks the server how many events the server has, then sends any events that it has that the server doesn't.
//!   2. The user's device tells the server what events it has, then the server responds with the events that the user's device doesn't have.
//!
//! Sounds simple, but there are a few tricky parts that this library handles.

#[cfg(feature = "supabase")]
pub mod supabase;

#[cfg(feature = "opfs")]
pub mod opfs;

#[cfg(target_arch = "wasm32")]
#[cfg(feature = "indexeddb")]
pub mod indexeddb;

pub mod data_model;

use crate::data_model::{Event, Timestamped};

pub trait AppState: Sized {
    type Event: Event;

    fn apply_event(self, event: &Timestamped<Self::Event>) -> Self;
}
