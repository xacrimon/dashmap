 use crate::alloc::{sarc_add_copy, sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::{Element, ElementGuard};
use crate::recl::{defer, protected};
use crate::util::{
    derive_filter, get_tag, range_split, set_tag, u64_read_byte, u64_write_byte, unreachable,
    CircularRange, FastCounter, PtrTag, read_cache, set_cache
};
use crate::{likely, unlikely};
use std::borrow::Borrow;
use std::cmp;
use std::collections::LinkedList;
use std::hash::{BuildHasher, Hash, Hasher};
use std::mem;
use std::ops::Range;
use std::ptr::{self, NonNull};
use std::sync::atomic::{spin_loop_hint, AtomicPtr, AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

macro_rules! maybe_grow {
    ($s:expr) => {
        if likely!($s.cells_remaining.fetch_sub(1, Ordering::SeqCst) == 1) {
            $s.grow();
        }
    };
}

macro_rules! on_heap {
    ($object:expr) => {
        Box::into_raw(Box::new($object))
    };
}

macro_rules! reap_now {
    ($ptr:expr) => {
        unsafe {
            Box::from_raw($ptr);
        }
    };
}

macro_rules! reap_defer {
    ($era:expr, $ptr:expr) => {{
        defer($era, move || reap_now!($ptr));
    }};
}

fn do_hash(f: &impl BuildHasher, i: &(impl ?Sized + Hash)) -> u64 {
    let mut hasher = f.build_hasher();
    i.hash(&mut hasher);
    hasher.finish()
}

fn make_groups<K, V>(amount: usize) -> Box<[AtomicPtr<Bucket<K, V>>]> {
    (0..amount).map(|_| AtomicPtr::new(ptr::null_mut())).collect()
}

type Bucket<K, V> = ABox<Element<K, V>>;

const LOAD_FACTOR: f32 = 0.75;

struct ResizeCoordinator<K, V, S> {
    root_ptr: *mut AtomicPtr<BucketArray<K, V, S>>,
    old_table: NonNull<BucketArray<K, V, S>>,
    new_table: NonNull<BucketArray<K, V, S>>,
    task_list: Mutex<LinkedList<Range<usize>>>,
    running: AtomicU16,
}

impl<K: Eq + Hash, V, S: BuildHasher> ResizeCoordinator<K, V, S> {
    fn new(
        root_ptr: *mut AtomicPtr<BucketArray<K, V, S>>,
        old_table: *mut BucketArray<K, V, S>,
    ) -> Self {
        unsafe {
            let old_table = NonNull::new_unchecked(old_table);
            let old_slot_amount = old_table.as_ref().buckets.len();
            let hash_builder = old_table.as_ref().hash_builder.clone();
            let era = old_table.as_ref().era;
            let new_table = on_heap!(BucketArray::new(
                root_ptr,
                old_slot_amount * 2,
                era,
                hash_builder,
            ));
            let task_list = Mutex::new(range_split(0..old_slot_amount, 1024));

            Self {
                root_ptr,
                old_table,
                new_table: NonNull::new_unchecked(new_table),
                task_list,
                running: AtomicU16::new(0),
            }
        }
    }

    fn run(&self, era: usize) {
        self.work();
        self.wait();
        unsafe {
            if (*self.root_ptr).compare_and_swap(
                self.old_table.as_ptr(),
                self.new_table.as_ptr(),
                Ordering::SeqCst,
            ) == self.old_table.as_ptr()
            {
                let old_table_ptr = self.old_table.as_ptr();
                reap_defer!(era, old_table_ptr);
            }
        }
    }

    fn wait(&self) {
        while self.running.load(Ordering::SeqCst) != 0 {
            spin_loop_hint()
        }
    }

    fn work(&self) {
        self.running.fetch_add(1, Ordering::SeqCst);
        while let Some(range) = self.task_list.lock().unwrap().pop_front() {
            for idx in range {
                unsafe {
                    'inner: loop {
                        let bucket_ptr = self.old_table.as_ref().buckets[idx].load(Ordering::SeqCst);
                        if self.old_table.as_ref().buckets[idx].compare_and_swap(
                            bucket_ptr,
                            set_tag(bucket_ptr, PtrTag::Resize),
                            Ordering::SeqCst,
                        ) != bucket_ptr
                        {
                            continue 'inner;
                        }
                        sarc_add_copy(bucket_ptr);
                        self.new_table.as_ref().put_node(bucket_ptr);
                        break;
                    }
                }
            }
        }

        self.running.fetch_sub(1, Ordering::SeqCst);
    }
}


fn calc_cells_remaining(capacity: usize) -> usize {
    (LOAD_FACTOR * capacity as f32) as usize
}

struct BucketArray<K, V, S> {
    root_ptr: *mut AtomicPtr<BucketArray<K, V, S>>,
    cells_remaining: AtomicUsize,
    hash_builder: Arc<S>,
    era: usize,
    next: AtomicPtr<ResizeCoordinator<K, V, S>>,
    buckets: Box<[AtomicPtr<Bucket<K, V>>]>,
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(
        root_ptr: *mut AtomicPtr<BucketArray<K, V, S>>,
        capacity: usize,
        era: usize,
        hash_builder: Arc<S>,
    ) -> Self {
        debug_assert!(capacity.is_power_of_two());
        let capacity = cmp::max(capacity, 16);
        let cells_remaining = calc_cells_remaining(capacity);
        let buckets = make_groups(capacity);

        Self {
            root_ptr,
            cells_remaining: AtomicUsize::new(cells_remaining),
            hash_builder,
            era,
            next: AtomicPtr::new(ptr::null_mut()),
            buckets,
        }
    }

    fn fetch_next<'a>(&self) -> Option<&'a Self> {
        let coordinator = self.next.load(Ordering::SeqCst);
        if likely!(coordinator.is_null()) {
            None
        } else {
            unsafe {
                (*coordinator).work();
                (*coordinator).wait();
                Some(&*(*coordinator).new_table.as_ptr())
            }
        }
    }

    fn grow(&self) {
        unsafe {
            let coordinator = self.next.load(Ordering::SeqCst);
            if !coordinator.is_null() {
                (*coordinator).work();
                (*coordinator).wait();
            } else {
                let new_coordinator =
                    on_heap!(ResizeCoordinator::new(self.root_ptr, mem::transmute(self),));
                let old =
                    self.next
                        .compare_and_swap(ptr::null_mut(), new_coordinator, Ordering::SeqCst);
                if old.is_null() {
                    (*new_coordinator).run(self.era);
                    reap_now!(new_coordinator);
                } else {
                    reap_now!(new_coordinator);
                    (*old).work();
                    (*old).wait();
                }
            }
        }
    }

    fn put_node(&self, mut node: *mut ABox<Element<K, V>>) -> Option<*mut ABox<Element<K, V>>> {
        if let Some(next) = self.fetch_next() {
            return next.put_node(node);
        }
        let node_data = sarc_deref(node);
        let hash = do_hash(&*self.hash_builder, &node_data.key);
        let idx_start = hash as usize % self.buckets.len();
        let filter = derive_filter(hash);
        node = set_cache(node, filter);
        for idx in CircularRange::new(0, self.buckets.len(), idx_start) {
            'inner: loop {
                let bucket_ptr = self.buckets[idx].load(Ordering::SeqCst);
                let cache = read_cache(bucket_ptr);
                match get_tag(bucket_ptr) {
                    PtrTag::None => (),
                    PtrTag::Resize => {
                        return self.fetch_next().unwrap().put_node(node);
                    }
                    PtrTag::Tombstone => {
                        if self.buckets[idx].compare_and_swap(bucket_ptr, node, Ordering::SeqCst) == bucket_ptr {
                           // Don't update cells_remaining since we are replacing a tombstone.
                           return None;
                        } else {
                            continue 'inner;
                        }
                    }
                }
                if bucket_ptr.is_null() {
                    if self.buckets[idx].compare_and_swap(bucket_ptr, node, Ordering::SeqCst) == bucket_ptr {
                        maybe_grow!(self);
                        return None;
                     } else {
                         continue 'inner;
                     }
                } else {
                    let bucket_data = sarc_deref(bucket_ptr);
                    if bucket_data.key == node_data.key {
                        if self.buckets[idx].compare_and_swap(bucket_ptr, node, Ordering::SeqCst) == bucket_ptr {
                            // Don't update cells_remaining since we are replacing an entry.
                            return None;
                         } else {
                             continue 'inner;
                         }
                    }
                }
            }
        }
        unreachable!()
    }
}
