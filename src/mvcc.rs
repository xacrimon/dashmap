#![allow(dead_code)]

use crate::table::Table;
use std::sync::atomic::{Ordering, AtomicUsize};
use std::sync::Mutex;
use std::hash::{Hash, BuildHasher};

type TxId = usize;
type AtomicTxId = AtomicUsize;

struct Record<V> {
    data: V,
    created: AtomicTxId,
    expired: AtomicTxId,
    next: Mutex<Option<*const Self>>,
}

pub struct MVCCTable<K, V, S> {
    next_txid: AtomicUsize,
    active: Table<TxId, (), S>,
    records: Table<K, Record<V>, S>,
}

pub struct Transaction<'a, K, V, S> {
    id: TxId,
    table: &'a MVCCTable<K, V, S>,
}
