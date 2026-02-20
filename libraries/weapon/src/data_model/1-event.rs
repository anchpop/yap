//! # Event
//! Events are the basic unit in Weapon's data model. The application state is simply the result of applying a sequence of events. Events are what is saved in persistent storage.
//! For robustness, events must be versionable. This means there is another type that is a "versioned" version, which is the one that is stored on disk/in supabase/etc.
//! This ensures that we can evolve the data model without breaking existing data.

use serde::{Serialize, de::DeserializeOwned};

pub trait Event: Sized + PartialOrd + Ord + Clone + Eq + PartialEq {
    /// The versioned form of this event, used for storage.
    /// This preserves the original version when loading/saving to disk.
    type Versioned: Sized + PartialOrd + Ord + Clone + Eq + PartialEq + Serialize + DeserializeOwned;

    /// Context needed for deversioning (e.g., string interning, lookup tables).
    /// Use `()` if no context is needed.
    type Context;

    /// Convert this event to its versioned form for storage.
    fn to_versioned(&self) -> Self::Versioned;

    /// Convert from versioned form back to the current event type.
    /// Context is provided for conversions that need external data.
    /// Returns None if this versioned event should be skipped (e.g., no-op migrations).
    fn from_versioned(versioned: &Self::Versioned, context: &Self::Context) -> Option<Self>;
}
