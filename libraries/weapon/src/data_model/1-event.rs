//! # Event
//! Events are the basic unit in Weapon's data model. The application state is simply the result of applying a sequence of events. Events are what is saved in persistent storage.
//! For robustness, events must be versionable. This means there is another type that is a "versioned" version, which is the one that is stored on disk/in supabase/etc.
//! This ensures that we can evolve the data model without breaking existing data.

pub trait Event: Sized + PartialOrd + Ord + Clone + Eq + PartialEq {
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error>;
    fn from_json(json: &serde_json::Value) -> Result<Self, serde_json::Error>;
}
