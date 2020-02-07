#![allow(dead_code)]

pub mod element;
pub mod table;

use crossbeam_epoch::pin;
use element::ElementReadGuard;
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::fmt::Debug;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;
use table::{do_hash, Table};

pub struct DashMap<K, V, S = RandomState> {
    table: Table<K, V, S>,
    hash_builder: Arc<S>,
}

impl<K: Eq + Hash + Debug, V> DashMap<K, V, RandomState> {
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<K: Eq + Hash + Debug, V, S: BuildHasher> DashMap<K, V, S> {
    pub fn with_hasher(hash_builder: S) -> Self {
        Self::with_capacity_and_hasher(0, hash_builder)
    }

    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let hash_builder = Arc::new(hash_builder);
        let table = Table::new(capacity, hash_builder.clone());

        Self {
            table,
            hash_builder,
        }
    }

    pub fn batch<T>(&self, f: impl FnOnce(&Self) -> T) -> T {
        let guard = pin();
        let r = f(self);
        guard.defer(|| ());
        r
    }

    pub fn insert(&self, key: K, value: V) {
        let hash = do_hash(&*self.hash_builder, &key);
        self.table.insert(key, hash, value);
    }

    pub fn get<'a, Q>(&'a self, key: &Q) -> Option<ElementReadGuard<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.get(key)
    }
}
