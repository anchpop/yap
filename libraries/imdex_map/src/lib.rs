//! Provides a map that maintains insertion order and prevents duplicate keys.
//! Built on top of im for efficient cloning and immutable operations.

#![allow(unused)]

use im::{HashMap, Vector};
use std::hash::Hash;
use std::iter::FromIterator;

/// An ordered map that maintains insertion order and prevents duplicate keys.
/// Built on top of im for efficient cloning and immutable operations.
#[derive(Clone)]
pub struct IndexMap<K: Hash + Eq + Clone, V: Clone> {
    // Maps keys to their index in the order vector
    indices: HashMap<K, usize>,
    // Maintains insertion order of key-value pairs
    order: Vector<(K, V)>,
}

impl<K: Clone + Hash + Eq, V: Clone> IndexMap<K, V> {
    /// Creates a new empty IndexMap
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
            order: Vector::new(),
        }
    }

    /// Returns the number of elements in the map
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Returns true if the map contains no elements
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Inserts a key-value pair into the map.
    /// If the key already exists, updates the value and returns the old value.
    /// Otherwise, returns None.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(&index) = self.indices.get(&key) {
            // Key already exists, update the value
            let old_pair = self.order.get(index)?;
            let old_value = old_pair.1.clone();
            self.order = self.order.update(index, (key, value));
            Some(old_value)
        } else {
            // New key, append to the end
            let index = self.order.len();
            self.indices = self.indices.update(key.clone(), index);
            self.order.push_back((key, value));
            None
        }
    }

    /// Returns a reference to the value corresponding to the key
    pub fn get(&self, key: &K) -> Option<&V> {
        let index = *self.indices.get(key)?;
        self.order.get(index).map(|(_, v)| v)
    }

    /// Returns a mutable reference to the value corresponding to the key
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let index = *self.indices.get(key)?;
        self.order.get_mut(index).map(|(_, v)| v)
    }

    /// Returns true if the map contains a value for the specified key
    pub fn contains_key(&self, key: &K) -> bool {
        self.indices.contains_key(key)
    }

    /// Removes a key from the map, returning the value if the key was present
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = *self.indices.get(key)?;

        // Remove from indices
        self.indices = self.indices.without(key);

        // Remove from order and get the old value
        let (_, old_value) = self.order.get(index)?.clone();
        self.order.remove(index);

        // Update indices for all elements after the removed one
        let mut new_indices = self.indices.clone();
        for (k, v) in &self.indices {
            if *v > index {
                new_indices = new_indices.update(k.clone(), v - 1);
            }
        }
        self.indices = new_indices;

        Some(old_value)
    }

    /// Gets the key-value pair at the given index
    pub fn get_index(&self, index: usize) -> Option<(&K, &V)> {
        self.order.get(index).map(|(k, v)| (k, v))
    }

    /// A mutable version of get_index. Only the value is mutable
    pub fn get_index_mut(&mut self, index: usize) -> Option<(&K, &mut V)> {
        self.order.get_mut(index).map(|(k, v)| (&*k, v))
    }

    /// Returns an iterator over the key-value pairs in insertion order
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.order.iter().map(|(k, v)| (k, v))
    }

    /// Returns an iterator over the keys in insertion order
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.order.iter().map(|(k, _)| k)
    }

    /// Returns an iterator over the values in insertion order
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.order.iter().map(|(_, v)| v)
    }

    /// Clears the map, removing all key-value pairs
    pub fn clear(&mut self) {
        self.indices = HashMap::new();
        self.order = Vector::new();
    }

    /// Gets the given key's corresponding entry in the map for in-place manipulation.
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        if self.contains_key(&key) {
            Entry::Occupied(OccupiedEntry { key, map: self })
        } else {
            Entry::Vacant(VacantEntry { key, map: self })
        }
    }
}

/// A view into a single entry in a map, which may either be vacant or occupied.
pub enum Entry<'a, K: Clone + Hash + Eq, V: Clone> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, K, V>),
    /// A vacant entry.
    Vacant(VacantEntry<'a, K, V>),
}

impl<'a, K: Clone + Hash + Eq, V: Clone> Entry<'a, K, V> {
    /// Ensures a value is in the entry by inserting the default if empty,
    /// and returns a mutable reference to the value in the entry.
    pub fn or_insert(self, default: V) -> &'a mut V {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(default),
        }
    }

    /// Ensures a value is in the entry by inserting the result of the default function if empty,
    /// and returns a mutable reference to the value in the entry.
    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(default()),
        }
    }

    /// Provides in-place mutable access to an occupied entry before any potential inserts into the map.
    pub fn and_modify<F: FnOnce(&mut V)>(self, f: F) -> Self {
        match self {
            Entry::Occupied(mut entry) => {
                f(entry.get_mut());
                Entry::Occupied(entry)
            }
            Entry::Vacant(entry) => Entry::Vacant(entry),
        }
    }

    /// Returns a reference to this entry's key.
    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(entry) => &entry.key,
            Entry::Vacant(entry) => &entry.key,
        }
    }
}

/// A view into an occupied entry in an `IndexMap`.
pub struct OccupiedEntry<'a, K: Clone + Hash + Eq, V: Clone> {
    key: K,
    map: &'a mut IndexMap<K, V>,
}

impl<'a, K: Clone + Hash + Eq, V: Clone> OccupiedEntry<'a, K, V> {
    /// Gets a reference to the value in the entry.
    pub fn get(&self) -> &V {
        self.map.get(&self.key).expect("key must exist")
    }

    /// Gets a mutable reference to the value in the entry.
    pub fn get_mut(&mut self) -> &mut V {
        self.map.get_mut(&self.key).expect("key must exist")
    }

    /// Converts the entry into a mutable reference to its value.
    pub fn into_mut(self) -> &'a mut V {
        self.map.get_mut(&self.key).expect("key must exist")
    }

    /// Sets the value of the entry with the `OccupiedEntry`'s key,
    /// and returns the entry's old value.
    pub fn insert(&mut self, value: V) -> V {
        self.map
            .insert(self.key.clone(), value)
            .expect("key must exist")
    }

    /// Takes the value of the entry out of the map, and returns it.
    pub fn remove(self) -> V {
        self.map.remove(&self.key).expect("key must exist")
    }
}

/// A view into a vacant entry in an `IndexMap`.
pub struct VacantEntry<'a, K: Clone + Hash + Eq, V: Clone> {
    key: K,
    map: &'a mut IndexMap<K, V>,
}

impl<'a, K: Clone + Hash + Eq, V: Clone> VacantEntry<'a, K, V> {
    /// Sets the value of the entry with the `VacantEntry`'s key,
    /// and returns a mutable reference to it.
    pub fn insert(self, value: V) -> &'a mut V {
        self.map.insert(self.key.clone(), value);
        self.map.get_mut(&self.key).expect("key was just inserted")
    }
}

impl<K: Clone + Hash + Eq, V: Clone> Default for IndexMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Clone + Hash + Eq, V: Clone> FromIterator<(K, V)> for IndexMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut map = Self::new();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

impl<K: Clone + Hash + Eq + std::fmt::Debug, V: Clone + std::fmt::Debug> std::fmt::Debug
    for IndexMap<K, V>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IndexMap {{")?;
        for (k, v) in self.iter() {
            write!(f, "{k:?}: {v:?}, ")?;
        }
        write!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insertion_order() {
        let mut map = IndexMap::new();
        map.insert("first", 1);
        map.insert("second", 2);
        map.insert("third", 3);

        let keys: Vec<_> = map.keys().cloned().collect();
        assert_eq!(keys, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_no_duplicates() {
        let mut map = IndexMap::new();
        assert_eq!(map.insert("key", 1), None);
        assert_eq!(map.insert("key", 2), Some(1));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&"key"), Some(&2));
    }

    #[test]
    fn test_index_access() {
        let mut map = IndexMap::new();
        map.insert("a", 10);
        map.insert("b", 20);
        map.insert("c", 30);

        assert_eq!(map.get_index(0), Some((&"a", &10)));
        assert_eq!(map.get_index(1), Some((&"b", &20)));
        assert_eq!(map.get_index(2), Some((&"c", &30)));
        assert_eq!(map.get_index(3), None);
    }

    #[test]
    fn test_remove() {
        let mut map = IndexMap::new();
        map.insert("a", 1);
        map.insert("b", 2);
        map.insert("c", 3);

        assert_eq!(map.remove(&"b"), Some(2));
        assert_eq!(map.len(), 2);

        let keys: Vec<_> = map.keys().cloned().collect();
        assert_eq!(keys, vec!["a", "c"]);

        // Check that indices are updated correctly
        assert_eq!(map.get_index(0), Some((&"a", &1)));
        assert_eq!(map.get_index(1), Some((&"c", &3)));
    }

    #[test]
    fn test_clone_efficiency() {
        let mut map1 = IndexMap::new();
        for i in 0..100 {
            map1.insert(i, i * 2);
        }

        // Clone should be cheap due to `im`
        let map2 = map1.clone();

        // Both maps should have the same content
        assert_eq!(map1.len(), map2.len());
        for i in 0..100 {
            assert_eq!(map1.get(&i), map2.get(&i));
        }
    }

    #[test]
    fn test_entry_api() {
        let mut map = IndexMap::new();

        // Test or_insert on vacant entry
        let value = map.entry("key1").or_insert(10);
        assert_eq!(*value, 10);
        *value = 20;
        assert_eq!(map.get(&"key1"), Some(&20));

        // Test or_insert on occupied entry
        let value = map.entry("key1").or_insert(30);
        assert_eq!(*value, 20);

        // Test or_insert_with
        let value = map.entry("key2").or_insert_with(|| 40);
        assert_eq!(*value, 40);

        // Test and_modify
        map.entry("key2").and_modify(|v| *v *= 2).or_insert(100);
        assert_eq!(map.get(&"key2"), Some(&80));

        // Test and_modify on vacant entry
        map.entry("key3").and_modify(|v| *v *= 2).or_insert(100);
        assert_eq!(map.get(&"key3"), Some(&100));

        // Test key() method
        let entry = map.entry("key4");
        assert_eq!(entry.key(), &"key4");
    }

    #[test]
    fn test_occupied_entry() {
        let mut map = IndexMap::new();
        map.insert("key", 10);

        if let Entry::Occupied(mut entry) = map.entry("key") {
            assert_eq!(entry.get(), &10);
            *entry.get_mut() = 20;
            assert_eq!(entry.get(), &20);

            let old_value = entry.insert(30);
            assert_eq!(old_value, 20);
            assert_eq!(entry.get(), &30);
        } else {
            panic!("Expected occupied entry");
        }
    }

    #[test]
    fn test_vacant_entry() {
        let mut map: IndexMap<&str, i32> = IndexMap::new();

        if let Entry::Vacant(entry) = map.entry("key") {
            let value_ref = entry.insert(42);
            assert_eq!(*value_ref, 42);
        } else {
            panic!("Expected vacant entry");
        }

        assert_eq!(map.get(&"key"), Some(&42));
    }
}
