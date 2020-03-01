#![allow(dead_code)]

mod alloc;
mod element;
mod recl;
mod table;
mod util;

pub use element::ElementGuard;
use recl::{new_era, purge_era};
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;
use table::{do_hash, Table};

pub struct DashMap<K, V, S = RandomState> {
    table: Table<K, V, S>,
    era: usize,
    hash_builder: Arc<S>,
}

impl<K: Eq + Hash, V> DashMap<K, V, RandomState> {
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> DashMap<K, V, S> {
    pub fn with_hasher(hash_builder: S) -> Self {
        Self::with_capacity_and_hasher(0, hash_builder)
    }

    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let hash_builder = Arc::new(hash_builder);
        let era = new_era();
        let table = Table::new(capacity, hash_builder.clone(), era);

        Self {
            table,
            era,
            hash_builder,
        }
    }

    pub fn insert(&self, key: K, value: V) -> bool {
        let hash = do_hash(&*self.hash_builder, &key);
        self.table.insert(key, hash, value)
    }

    pub fn get<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.get(key)
    }

    pub fn extract<T, Q, F>(&self, key: &Q, do_extract: F) -> Option<T>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnOnce(&K, &V) -> T,
    {
        self.table.extract(key, do_extract)
    }

    pub fn remove<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.remove(key)
    }
}

impl<K: Eq + Hash + Clone, V, S: BuildHasher> DashMap<K, V, S> {
    pub fn update<Q, F>(&self, key: &Q, mut do_update: F) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        self.table.optimistic_update(key, &mut do_update)
    }
}

impl<K, V, S> Drop for DashMap<K, V, S> {
    fn drop(&mut self) {
        purge_era(self.era);
    }
}
