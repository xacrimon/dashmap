#![allow(dead_code)]

use crate::table::Table;
use std::sync::atomic::{Ordering, AtomicUsize};
use std::sync::Mutex;
use std::hash::{Hash, BuildHasher};

struct Record<V> {
    data: V,
    created: AtomicUsize,
    expired: AtomicUsize,
    next: Mutex<Option<*const Self>>,
}

#[derive(Hash)]
struct CompositeKey<K> {
    id: usize,
    key: K,
}

pub struct MVCCTable<K, V, S> {
    next_txid: AtomicUsize,
    table: Table<CompositeKey<K>, Record<V>, S>,
}
