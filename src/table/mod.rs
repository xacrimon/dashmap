mod entry_manager;
mod recl;
mod spec;

use crate::circular_range::CircularRange;
use entry_manager::{EntryManager, NewEntryState};
use std::borrow::Borrow;
use std::cmp;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::{spin_loop_hint, AtomicPtr, AtomicUsize, Ordering};
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
        if let Some(next) = self.next() {
            return next.cas(search_key, f);
        }

        let hash = do_hash(&*self.hasher_builder, search_key);
        let slots_amount = self.buckets.len();
        let start_idx = hash as usize & (slots_amount - 1);

        for idx in CircularRange::new(slots_amount, start_idx) {
            let atomic_entry = &self.buckets[idx];
            let bucket_pointer = atomic_entry.load(Ordering::SeqCst);

            if M::is_null(bucket_pointer) {
                break;
            } else if M::is_tombstone(bucket_pointer) {
                continue;
            } else if M::is_resize(bucket_pointer) {
                if let Some(next) = self.next() {
                    return next.cas(search_key, f);
                } else {
                    break;
                }
            }

            if M::eq(bucket_pointer, search_key, hash) {
                return M::cas(atomic_entry, |_loaded_bucket_ptr, maybe_existing| {
                    match f(maybe_existing) {
                        CasOutput::Keep => NewEntryState::Keep,
                        CasOutput::Empty => NewEntryState::Empty,
                        CasOutput::New(key, value) => todo!(),
                    }
                });
            }
        }

        false
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
        while self.running.load(Ordering::SeqCst) != 0 {
            spin_loop_hint();
        }
    }

    fn work(&self) {
        todo!()
    }
}
