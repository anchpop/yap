use rustc_hash::FxHashMap;
use std::hash::Hash;

/// An ordered map that maintains insertion order and prevents duplicate keys.
#[derive(
    Clone, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct IndexMap<K: Hash + Eq + Clone, V: Clone> {
    indices: FxHashMap<K, usize>,
    order: Vec<(K, V)>,
}

impl<K: Clone + Hash + Eq, V: Clone> IndexMap<K, V> {
    pub fn new() -> Self {
        Self {
            indices: FxHashMap::default(),
            order: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Returns the insertion index of the key, if present
    pub fn get_index_of(&self, key: &K) -> Option<usize> {
        self.indices.get(key).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Inserts a key-value pair into the map.
    /// If the key already exists, updates the value and returns the old value.
    /// Otherwise, returns None.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(&index) = self.indices.get(&key) {
            let old_pair = self.order.get(index)?;
            let old_value = old_pair.1.clone();
            self.order[index] = (key, value);
            Some(old_value)
        } else {
            let index = self.order.len();
            self.indices.insert(key.clone(), index);
            self.order.push((key, value));
            None
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let index = *self.indices.get(key)?;
        self.order.get(index).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let index = *self.indices.get(key)?;
        self.order.get_mut(index).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.indices.contains_key(key)
    }

    pub fn get_index(&self, index: usize) -> Option<(&K, &V)> {
        self.order.get(index).map(|(k, v)| (k, v))
    }

    /// A mutable version of get_index. Only the value is mutable
    pub fn get_index_mut(&mut self, index: usize) -> Option<(&K, &mut V)> {
        self.order.get_mut(index).map(|(k, v)| (&*k, v))
    }

    /// Returns an iterator over the key-value pairs in insertion order
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&K, &V)> {
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

    pub fn clear(&mut self) {
        self.indices = FxHashMap::default();
        self.order = Vec::new();
    }

    /// Removes a key from the map, returning the value at the key if the key was previously in the map.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = *self.indices.get(key)?;
        self.indices.remove(key);
        let (_, value) = self.order.remove(index);

        for (_, idx) in self.indices.iter_mut() {
            if *idx > index {
                *idx -= 1;
            }
        }

        Some(value)
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
    Occupied(OccupiedEntry<'a, K, V>),
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

    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(entry) => &entry.key,
            Entry::Vacant(entry) => &entry.key,
        }
    }
}

pub struct OccupiedEntry<'a, K: Clone + Hash + Eq, V: Clone> {
    key: K,
    map: &'a mut IndexMap<K, V>,
}

impl<'a, K: Clone + Hash + Eq, V: Clone> OccupiedEntry<'a, K, V> {
    pub fn get(&self) -> &V {
        self.map.get(&self.key).expect("key must exist")
    }

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

        assert_eq!(map.get_index(0), Some((&"a", &1)));
        assert_eq!(map.get_index(1), Some((&"c", &3)));
    }

    #[test]
    fn test_entry_api() {
        let mut map = IndexMap::new();

        let value = map.entry("key1").or_insert(10);
        assert_eq!(*value, 10);
        *value = 20;
        assert_eq!(map.get(&"key1"), Some(&20));

        let value = map.entry("key1").or_insert(30);
        assert_eq!(*value, 20);

        let value = map.entry("key2").or_insert_with(|| 40);
        assert_eq!(*value, 40);

        map.entry("key2").and_modify(|v| *v *= 2).or_insert(100);
        assert_eq!(map.get(&"key2"), Some(&80));

        map.entry("key3").and_modify(|v| *v *= 2).or_insert(100);
        assert_eq!(map.get(&"key3"), Some(&100));

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
