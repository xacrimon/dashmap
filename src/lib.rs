#![allow(unused_unsafe)]

mod alloc;
mod element;
mod iter_shim;
mod spec;
mod table;
mod util;

pub use element::ElementGuard;
pub use iter_shim::Iter;
use spec::Table;
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};
use table::Table as TableTrait;
use std::fmt;
use std::iter::FromIterator;

/// DashMap is an implementation of a concurrent associative array/hashmap in Rust.
///
/// This library attempts to be a replacement for most uses of `RwLock<HashMap<K, V>>` but does not cover
/// all scenarios such as multi key serializable operations that are possible with `RwLock<HashMap<K, V>>`.
/// Instead we prefer speed and leave more complex constructs to libraries that can build on top of this.
///
/// In essence, this is meant to be a high performance core that can be built upon.
///
/// In contrast to v3 and prior versions. You cannot deadlock this.
/// You are free to run any combination of operations and
/// keep any combination of guards and be guaranteed not to deadlock.
pub struct DashMap<K, V, S = RandomState> {
    table: Table<K, V, S>,
}

impl<K: Eq + Hash + 'static, V: 'static> DashMap<K, V, RandomState> {
    /// Creates a new DashMap.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    ///
    /// let reviews = DashMap::new();
    /// reviews.insert("Veloren", "What a fantastic game!");
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity_and_hasher(0, RandomState::new())
    }

    /// Creates a new DashMap with a specified starting capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    ///
    /// let mappings = DashMap::with_capacity(2);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> DashMap<K, V, S> {
    /// Creates a new DashMap with a capacity of 0 and the provided hasher.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let reviews = DashMap::with_hasher(s);
    /// reviews.insert("Veloren", "What a fantastic game!");
    /// ```
    #[inline]
    pub fn with_hasher(build_hasher: S) -> Self {
        Self::with_capacity_and_hasher(0, build_hasher)
    }

    /// Creates a new DashMap with a specified starting capacity and hasher.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let mappings = DashMap::with_capacity_and_hasher(2, s);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    #[inline]
    pub fn with_capacity_and_hasher(capacity: usize, build_hasher: S) -> Self {
        let table = Table::new(capacity, build_hasher);

        Self { table }
    }

    /// Inserts a key and a value into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    ///
    /// let map = DashMap::new();
    /// map.insert("I am the key!", "And I am the value!");
    /// ```
    #[inline]
    pub fn insert(&self, key: K, value: V) -> bool {
        self.table.insert(key, value)
    }

    /// Inserts a key and a value into the map and returns a guard to the new entry.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    ///
    /// let map = DashMap::new();
    /// let new_entry = map.insert_and_get("I am the key!", "And I am the value!");
    /// assert!(*new_entry.value() == "And I am the value!")
    /// ```
    #[inline]
    pub fn insert_and_get(&self, key: K, value: V) -> ElementGuard<K, V> {
        self.table.insert_and_get(key, value)
    }

    /// Inserts a key and a value into the map and returns a guard to the replaced entry if there was one.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    ///
    /// let map = DashMap::new();
    /// map.insert("I am the key!", "I'm the old value!");
    /// let maybe_old_entry = map.replace("I am the key!", "And I am the value!");
    /// assert!(maybe_old_entry.is_some());
    /// let old_entry = maybe_old_entry.unwrap();
    /// assert!(*old_entry.value() == "I'm the old value!");
    /// ```
    #[inline]
    pub fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>> {
        self.table.replace(key, value)
    }

    /// Get the entry of a key if it exists in the map.
    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.get(key)
    }

    /// Check if the map contains a specific key.
    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.contains_key(key)
    }

    /// Removes an entry from the map.
    /// Returns true if the key existed and the entry was removed. Otherwise returns false.
    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.remove(key)
    }

    /// Removes an entry from the map if the conditional returns true.
    /// Returns true if the key existed and the entry was removed. Otherwise returns false.
    #[inline]
    pub fn remove_if<Q, P>(&self, key: &Q, predicate: P) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        P: FnMut(&K, &V) -> bool,
    {
        let mut predicate = predicate;
        self.table.remove_if(key, &mut predicate)
    }

    /// Removes an entry from the map.
    /// Returns the entry if it existed in the map. Otherwise returns `None`.
    #[inline]
    pub fn remove_take<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.remove_take(key)
    }

    /// Removes an entry from the map if the conditional returns true.
    /// Returns the entry if it existed in the map. Otherwise returns `None`.
    #[inline]
    pub fn remove_if_take<Q, P>(&self, key: &Q, predicate: P) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        P: FnMut(&K, &V) -> bool,
    {
        let mut predicate = predicate;
        self.table.remove_if_take(key, &mut predicate)
    }

    /// Run a closure on an entry in the map.
    /// If the key existed the function's output is returned. Otherwise returns `None`.
    #[inline]
    pub fn extract<T, Q, F>(&self, search_key: &Q, do_extract: F) -> Option<T>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnOnce(&K, &V) -> T,
    {
        self.table.extract(search_key, do_extract)
    }

    /// Update the value of a key in the map by supplying a mutation closure.
    /// Returns true if the key existed. Otherwise returns false.
    #[inline]
    pub fn update<Q, F>(&self, search_key: &Q, do_update: F) -> bool
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        let mut do_update = do_update;
        self.table.update(search_key, &mut do_update)
    }

    /// Update the value of a key in the map by supplying a mutation closure.
    /// Returns the updated entry of the key if the key existed. Otherwise returns false.
    #[inline]
    pub fn update_get<Q, F>(&self, search_key: &Q, do_update: F) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        let mut do_update = do_update;
        self.table.update_get(search_key, &mut do_update)
    }

    /// Create an iterator over all entries in the map.
    /// This does not take a snapshot of the map and thus changes
    /// during the lifetime of the iterator may or may not become visible in the iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    ///
    /// let words = DashMap::new();
    /// words.insert("hello", "world");
    /// words.insert("macn", "cheese");
    /// assert_eq!(words.iter().count(), 2);
    /// ```
    #[inline]
    pub fn iter(&self) -> Iter<K, V> {
        let internal = Box::new(self.table.iter());
        Iter::new(internal)
    }

    /// Retain elements that the filter closure returns true for.
    #[inline]
    pub fn retain(&self, mut predicate: impl FnMut(&K, &V) -> bool) {
        self.table.retain(&mut predicate)
    }

    /// Clear all entries in the map.
    #[inline]
    pub fn clear(&self) {
        self.table.clear();
    }

    /// Get the amount of entries in the map.
    #[inline]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Checks if the map is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the capacity of the map. That is the maximum amount of entries before a reallocation is needed.
    /// The backend implementation cannot always know the capacity. If this function returns 0, the capacity is unknown.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.table.capacity()
    }

    /// Create a map from an iterator over key + value pairs.
    #[inline]
    pub fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
        S: Default,
    {
        let map = DashMap::with_hasher(S::default());

        for (key, value) in iter {
            map.insert(key, value);
        }

        map
    }

    /// Extend the map with an iterator over key + value pairs.
    #[inline]
    pub fn extend<T>(&self, iter: T)
    where
        T: IntoIterator<Item = (K, V)>,
    {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<K: Eq + Hash + 'static, V: 'static> Default for DashMap<K, V, RandomState> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + 'static + fmt::Debug, V: 'static + fmt::Debug, S: BuildHasher + 'static> fmt::Debug for DashMap<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let guards: Vec<_> = self.iter().collect();
        f.debug_map().entries(guards.iter().map(|guard| (guard.key(), guard.value()))).finish()
    }
}

impl<K: Eq + Hash + 'static, V: 'static> FromIterator<(K, V)> for DashMap<K, V, RandomState> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>
    {
        Self::from_iter(iter)
    }
}
