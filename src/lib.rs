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

/// DashMap is an implementation of a concurrent associative array/hashmap in Rust.
///
/// DashMap tries to implement an easy to use API similar to `std::collections::HashMap`
/// with some slight changes to handle concurrency.
///
/// DashMap tries to be very simple to use and to be a direct replacement for `RwLock<HashMap<K, V, S>>`.
/// To accomplish these all methods take `&self` instead modifying methods taking `&mut self`.
/// This allows you to put a DashMap in an `Arc<T>` and share it between threads while being able to modify it.
///
/// DashMap does not supply methods for multi key atomic transactions and other high level constructs.
/// We want to build a fast base layer other libraries can use.
pub struct DashMap<K, V, S = RandomState> {
    table: Table<K, V, S>,
}

impl<K: Eq + Hash + 'static, V: 'static> DashMap<K, V, RandomState> {
    /// Creates a new DashMap with a capacity of 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashMap;
    ///
    /// let reviews = DashMap::new();
    /// reviews.insert("Veloren", "What a fantastic game!");
    /// ```
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
    pub fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>> {
        self.table.replace(key, value)
    }

    /// Get the entry of a key if it exists in the map.
    pub fn get<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.get(key)
    }

    /// Check if the map contains a specific key.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.contains_key(key)
    }

    /// Removes an entry from the map.
    /// Returns true if the key existed and the entry was removed. Otherwise returns false.
    pub fn remove<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.remove(key)
    }

    /// Removes an entry from the map if the conditional returns true.
    /// Returns true if the key existed and the entry was removed. Otherwise returns false.
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
    pub fn remove_take<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.remove_take(key)
    }

    /// Removes an entry from the map if the conditional returns true.
    /// Returns the entry if it existed in the map. Otherwise returns `None`.
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
    pub fn iter(&self) -> Iter<K, V> {
        let internal = Box::new(self.table.iter());
        Iter::new(internal)
    }

    /// Retain elements that the filter closure returns true for.
    pub fn retain(&self, mut predicate: impl FnMut(&K, &V) -> bool) {
        self.table.retain(&mut predicate)
    }

    /// Clear all entries in the map.
    pub fn clear(&self) {
        self.table.clear();
    }

    /// Get the amount of entries in the map.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Checks if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the capacity of the map. That is the maximum amount of entries before a reallocation is needed.
    pub fn capacity(&self) -> usize {
        self.table.capacity()
    }
}
