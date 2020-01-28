#![allow(dead_code)]

pub mod element;
pub mod table;

use std::collections::hash_map::RandomState;
use table::{Table, make_shift, hash2idx, do_hash};
use std::sync::Arc;
use std::hash::{Hash, BuildHasher};
use std::cmp;
use crossbeam_epoch::pin;

const TABLES_PER_MAP: usize = 1;

pub struct DashMap<K, V, S = RandomState> {
    tables: [Table<K, V, S>; TABLES_PER_MAP],
    h2i_shift: usize,
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
        let capacity_per_table = cmp::max(capacity, 4 * TABLES_PER_MAP) / TABLES_PER_MAP;
        let h2i_shift = make_shift(TABLES_PER_MAP);
        let table_iter = (0..TABLES_PER_MAP).map(|_| Table::new(capacity_per_table, hash_builder.clone()));
        let tables = array_init::from_iter(table_iter).unwrap();
        
        Self {
            tables,
            h2i_shift,
        }
    }

    pub fn batch<T>(&self, f: impl FnOnce(&Self) -> T) -> T {
        let guard = pin();
        let r = f(self);
        guard.defer(|| ());
        r
    }
}
