//! # DirtyTracker
//! A DirtyTracker is a wrapper around any type, that adds a "dirty" flag. This is used to track whether the event stream has been updated.
//! This allows consumers of the event stream to know when to re-compute derived data.

use std::ops::{Deref, DerefMut};

use crate::data_model::ListenerKey;

#[derive(Clone, Debug)]
pub enum DirtyState {
    /// Not dirty, no pending notifications
    Clean,
    /// Dirty, notify all listeners except the specified one
    DirtyExcept(ListenerKey),
    /// Dirty, notify all listeners
    DirtyAll,
}

#[derive(Clone)]
pub struct DirtyTracker<Store> {
    store: Store,
    /// Tracks whether there are pending notifications and who should be notified
    pub dirty_state: DirtyState,
    loaded_at_least_once: bool,
}

impl<Store: Default> Default for DirtyTracker<Store> {
    fn default() -> Self {
        Self {
            store: Default::default(),

            // Creating a stream is an action that warrants a notification.
            dirty_state: DirtyState::DirtyAll,
            loaded_at_least_once: false,
        }
    }
}

/// Smart pointer that marks the store as dirty when dereferenced mutably
pub struct DirtyOnDerefMut<'a, Store> {
    dirty_tracker: &'a mut Store,
    dirty_state: &'a mut DirtyState,
    modifier: Option<ListenerKey>,
}

impl<'a, Store> Deref for DirtyOnDerefMut<'a, Store> {
    type Target = Store;

    fn deref(&self) -> &Self::Target {
        self.dirty_tracker
    }
}

impl<'a, Store> DerefMut for DirtyOnDerefMut<'a, Store> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mark_dirty();
        self.dirty_tracker
    }
}

impl<'a, Store> DirtyOnDerefMut<'a, Store> {
    fn mark_dirty(&mut self) {
        use DirtyState::*;
        *self.dirty_state = match (&self.dirty_state, self.modifier) {
            (Clean, Some(key)) => DirtyExcept(key),
            (DirtyExcept(key1), Some(key2)) if key1 == &key2 => DirtyExcept(*key1),
            (Clean, None) => DirtyAll,
            (DirtyExcept(_), _) | (DirtyAll, _) => DirtyAll,
        };
    }

    pub(crate) fn map<NewStore>(
        self,
        f: fn(&mut Store) -> &mut NewStore,
    ) -> DirtyOnDerefMut<'a, NewStore> {
        DirtyOnDerefMut {
            dirty_tracker: f(self.dirty_tracker),
            dirty_state: self.dirty_state,
            modifier: self.modifier,
        }
    }
}

impl<Store> DirtyTracker<Store> {
    /// Returns true if the `loaded` marker was changed
    pub(crate) fn mark_loaded(&mut self, modifier: Option<ListenerKey>) -> bool {
        if !self.loaded_at_least_once {
            self.loaded_at_least_once = true;
            self.store_mut(modifier).mark_dirty();
            true
        } else {
            false
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub(crate) fn store_mut(&mut self, modifier: Option<ListenerKey>) -> DirtyOnDerefMut<Store> {
        DirtyOnDerefMut {
            dirty_tracker: &mut self.store,
            dirty_state: &mut self.dirty_state,
            modifier,
        }
    }

    pub fn loaded_at_least_once(&self) -> bool {
        self.loaded_at_least_once
    }

    pub fn map<NewStore>(self, f: impl FnOnce(Store) -> NewStore) -> DirtyTracker<NewStore> {
        DirtyTracker {
            store: f(self.store),
            dirty_state: self.dirty_state,
            loaded_at_least_once: self.loaded_at_least_once,
        }
    }
}
