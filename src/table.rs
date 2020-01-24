use super::element::*;
use crossbeam_epoch::{Atomic, Guard, Owned, Shared};
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct BucketArray<K, V, S> {
    remaining_cells: AtomicUsize,
    hash_builder: Arc<S>,
    buckets: Box<[Atomic<Element<K, V>>]>,
    next: Atomic<Self>,
}
