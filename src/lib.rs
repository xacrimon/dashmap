#![allow(dead_code)]
#![allow(unused_imports)]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]

mod alloc;
mod element;
mod recl;
mod table;
mod util;

use element::ElementGuard;
use recl::{new_era, purge_era};
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;
use table::Table;

pub struct DashMap<K, V, S = RandomState> {
    era: usize,
    table: Table<K, V, S>,
}

impl<K: Eq + Hash, V> DashMap<K, V, RandomState> {
    pub fn new() -> Self {
        Self::with_capacity_and_hasher(0, RandomState::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> DashMap<K, V, S> {
    pub fn with_hasher(build_hasher: S) -> Self {
        Self::with_capacity_and_hasher(0, build_hasher)
    }

    pub fn with_capacity_and_hasher(capacity: usize, build_hasher: S) -> Self {
        let era = new_era();
        let table = Table::new(capacity, era, Arc::new(build_hasher));

        Self { era, table }
    }

    pub fn insert(&self, key: K, value: V) -> bool {
        self.table.insert(key, value)
    }

    pub fn insert_and_get(&self, key: K, value: V) -> ElementGuard<K, V> {
        self.table.insert_and_get(key, value)
    }

    pub fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>> {
        self.table.replace(key, value)
    }

    pub fn get<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.get(key)
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.contains_key(key)
    }

    pub fn remove<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.remove(key)
    }

    pub fn remove_if<Q, P>(&self, key: &Q, predicate: P) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        P: FnMut(&K, &V) -> bool,
    {
        let mut predicate = predicate;
        self.table.remove_if(key, &mut predicate)
    }

    pub fn remove_take<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.table.remove_take(key)
    }

    pub fn remove_if_take<Q, P>(&self, key: &Q, predicate: P) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        P: FnMut(&K, &V) -> bool,
    {
        let mut predicate = predicate;
        self.table.remove_if_take(key, &mut predicate)
    }
}
