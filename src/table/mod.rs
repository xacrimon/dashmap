mod entry_manager;
mod recl;
mod spec;

use entry_manager::{EntryManager, NewEntryState};
use std::borrow::Borrow;
use std::cmp;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const LOAD_FACTOR_THRESHOLD: f32 = 0.75;

fn ba_new_capacity(capacity: usize) -> usize {
    let threshold_adjusted = (capacity as f32 * LOAD_FACTOR_THRESHOLD.recip()) as usize;
    cmp::max(capacity, 16).next_power_of_two()
}

fn cells_remaining(capacity: usize) -> usize {
    (LOAD_FACTOR_THRESHOLD * capacity as f32) as usize
}

fn do_hash<S: BuildHasher, T: ?Sized + Hash>(f: &S, i: &T) -> u64 {
    let mut hasher = f.build_hasher();
    i.hash(&mut hasher);
    hasher.finish()
}

pub enum CasOutput<K: 'static + Eq + Hash, V: 'static> {
    Empty,
    Keep,
    New(K, V),
}

struct BucketArray<M, S> {
    root_array: *mut AtomicPtr<Self>,
    cells_remaining: usize,
    hasher_builder: Arc<S>,
    next: AtomicPtr<ResizeCoordinator<M, S>>,
    buckets: Box<[AtomicUsize]>,
}

impl<M: EntryManager, S: BuildHasher> BucketArray<M, S> {
    fn new(root_array: *mut AtomicPtr<Self>, capacity: usize, hasher_builder: Arc<S>) -> Self {
        let capacity = ba_new_capacity(capacity);
        let cells_remaining = cells_remaining(capacity);
        let next = AtomicPtr::new(0 as _);
        let buckets = (0..capacity).map(|_| M::empty()).collect();

        Self {
            root_array,
            cells_remaining,
            hasher_builder,
            next,
            buckets,
        }
    }

    fn next<'a>(&self) -> Option<&'a Self> {
        let coordinator = self.next.load(Ordering::Relaxed);

        if coordinator.is_null() {
            None
        } else {
            let coordinator = unsafe { &*coordinator };
            coordinator.work();
            coordinator.wait();
            Some(unsafe { &*coordinator.new_table })
        }
    }

    fn cas<Q, F>(&self, search_key: &Q, f: F) -> bool
    where
        M::K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnOnce(Option<(*const M::K, *const M::V)>) -> CasOutput<M::K, M::V>,
    {
        let hash = do_hash(&*self.hasher_builder, search_key);
        let mut idx = hash & (self.buckets.len() as u64 - 1);

        todo!()
    }
}

struct ResizeCoordinator<M, S> {
    root_array: *mut AtomicPtr<BucketArray<M, S>>,
    old_table: *mut BucketArray<M, S>,
    new_table: *mut BucketArray<M, S>,
    task_list: Mutex<Vec<Range<usize>>>,
    running: AtomicUsize,
}

impl<M: EntryManager, S: BuildHasher> ResizeCoordinator<M, S> {
    fn wait(&self) {
        todo!()
    }

    fn work(&self) {
        todo!()
    }
}
