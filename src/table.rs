use super::element::*;
use crossbeam_epoch::{Atomic, Owned, Shared, Guard};
use std::sync::atomic::{Ordering, AtomicUsize};
use std::hash::BuildHasher;
use std::sync::Arc;

pub struct BucketArray<K, V, S> {
    remaining_cells: AtomicUsize,
    hash_builder: Arc<S>,
    buckets: Box<[Atomic<Element<K, V>>]>,
    next: Atomic<Self>,
}
