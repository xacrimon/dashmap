#![allow(dead_code)]
#![allow(unused_imports)]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]

mod alloc;
mod element;
mod recl;
mod table;
mod util;

use std::collections::hash_map::RandomState;
use table::Table;
use element::Element;
use recl::{new_era, purge_era};
use std::hash::{Hash, BuildHasher};
use std::borrow::Borrow;
use std::sync::Arc;

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

        Self {
            era,
            table,
        }
    }
}
