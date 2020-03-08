use crate::alloc::{sarc_add_copy, sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::{Element, ElementGuard};
use crate::recl::{defer, protected};
use crate::util::{set_tag, u64_read_byte, u64_write_byte, PtrTag, get_tag};
use std::cmp;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

fn do_hash(f: &impl BuildHasher, i: &(impl ?Sized + Hash)) -> u64 {
    let mut hasher = f.build_hasher();
    i.hash(&mut hasher);
    hasher.finish()
}

fn make_groups<K, V>(amount: usize) -> Box<[G<K, V>]> {
    (0..amount).map(|_| Group::new()).collect()
}

struct Group<T> {
    cache: AtomicU64,
    nodes: [AtomicPtr<T>; 8],
}

impl<T> Group<T> {
    fn new() -> Self {
        Self {
            cache: AtomicU64::new(0),
            nodes: [
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
            ],
        }
    }

    fn probe(&self, filter: u8, mut apply: impl FnMut(*mut T)) -> bool {
        let cache = self.cache.load(Ordering::SeqCst);
        for i in 0..8 {
            if u64_read_byte(cache, i) == filter {
                let pointer = self.nodes[i].load(Ordering::SeqCst);
                apply(pointer);
                return true;
            }
        }
        return false;
    }

    fn try_publish(
        &self,
        i: usize,
        cache_maybe_current: u8,
        pointer_maybe_current: *mut T,
        cache_new: u8,
        pointer_new: *mut T,
    ) -> bool {
        let cache_all_current = self.cache.load(Ordering::SeqCst);
        let cache_current_sq = u64_write_byte(cache_all_current, i, cache_maybe_current);
        let updated_all_cache = u64_write_byte(cache_all_current, i, cache_new);
        if self
            .cache
            .compare_and_swap(cache_current_sq, updated_all_cache, Ordering::SeqCst)
            != cache_current_sq
        {
            return false;
        }
        if self.nodes[i].compare_and_swap(pointer_maybe_current, pointer_new, Ordering::SeqCst)
            != pointer_maybe_current
        {
            return false;
        }
        if self.cache.load(Ordering::SeqCst) != updated_all_cache {
            return false;
        }
        true
    }
}

type G<K, V> = Group<Element<K, V>>;

static LOAD_FACTOR: f32 = 0.75;

fn calc_cells_remaining(capacity: usize) -> usize {
    (LOAD_FACTOR * capacity as f32) as usize
}

fn calc_capacity(capacity: usize) -> usize {
    let fac = 1.0 / LOAD_FACTOR;
    (capacity as f32 * fac) as usize + 1
}

fn round_8mul(x: usize) -> usize {
    (x + 7) & !7
}

struct BucketArray<K, V, S> {
    cells_remaining: AtomicUsize,
    hash_builder: Arc<S>,
    era: usize,
    next: AtomicPtr<Self>,
    groups: Box<[G<K, V>]>,
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(capacity: usize, era: usize, hash_builder: Arc<S>) -> Self {
        let capacity = round_8mul(calc_capacity(cmp::max(capacity, 16)));
        let cells_remaining = calc_cells_remaining(capacity);
        let groups = make_groups(capacity / 8);

        Self {
            cells_remaining: AtomicUsize::new(cells_remaining),
            hash_builder,
            era,
            next: AtomicPtr::new(ptr::null_mut()),
            groups,
        }
    }

    fn slots_cap(&self) -> usize {
        self.groups.len() * 8
    }
}
