use super::element::*;
use crossbeam_epoch::{Atomic, Guard, Owned, Shared};
use std::hash::{Hash, BuildHasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::mem;
use std::iter;

const PTR_SIZE_BITS: usize = mem::size_of::<usize>() * 8;
const REDIRECT_TAG: usize = 5;

fn make_shift(x: usize) -> usize {
    debug_assert!(x.is_power_of_two());
    PTR_SIZE_BITS - x.trailing_zeros() as usize
}

fn make_buckets<K, V>(x: usize) -> Box<[Atomic<Element<K, V>>]> {
    iter::repeat(Atomic::null()).take(x).collect()
}

pub struct BucketArray<K, V, S> {
    remaining_cells: AtomicUsize,
    shift: usize,
    hash_builder: Arc<S>,
    buckets: Box<[Atomic<Element<K, V>>]>,
    next: Atomic<Self>,
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    pub fn new(capacity: usize, hash_builder: Arc<S>) -> Self {
        let remaining_cells = AtomicUsize::new(capacity * 3 / 4);
        let shift = make_shift(capacity);
        let buckets = make_buckets(capacity);

        Self {
            remaining_cells,
            shift,
            hash_builder,
            buckets,
            next: Atomic::null(),
        }
    }
}
