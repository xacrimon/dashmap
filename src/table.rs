use crate::alloc::{sarc_add_copy, sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::{Element, ElementGuard};
use crate::recl::{defer, protected};
use crate::util::{
    derive_filter, get_tag, range_split, set_tag, u64_read_byte, u64_write_byte, PtrTag,
};
use std::cmp;
use std::collections::LinkedList;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Range;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct ResizeCoordinator<K, V, S> {
    old_table: NonNull<BucketArray<K, V, S>>,
    new_table: Box<BucketArray<K, V, S>>,
    task_list: Mutex<LinkedList<Range<usize>>>,
    running: AtomicU16,
}

impl<K: Eq + Hash, V, S: BuildHasher> ResizeCoordinator<K, V, S> {
    pub fn new(old_table: *mut BucketArray<K, V, S>) -> Self {
        unsafe {
            let old_table = NonNull::new_unchecked(old_table);
            let old_group_amount = old_table.as_ref().group_amount();
            let hash_builder = old_table.as_ref().hash_builder.clone();
            let era = old_table.as_ref().era;
            let new_table = Box::new(BucketArray::new(
                old_group_amount * 16 * 2,
                era,
                hash_builder,
            ));
            let task_list = Mutex::new(range_split(0..old_group_amount, 128));

            Self {
                old_table,
                new_table,
                task_list,
                running: AtomicU16::new(0),
            }
        }
    }
}

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

struct BucketArray<K, V, S> {
    cells_remaining: AtomicUsize,
    hash_builder: Arc<S>,
    era: usize,
    next: AtomicPtr<Self>,
    groups: Box<[G<K, V>]>,
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(capacity: usize, era: usize, hash_builder: Arc<S>) -> Self {
        debug_assert!(capacity.is_power_of_two());
        let capacity = cmp::max(capacity, 16);
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

    fn group_amount(&self) -> usize {
        self.groups.len()
    }
}
