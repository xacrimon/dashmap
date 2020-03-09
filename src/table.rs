use crate::alloc::{sarc_add_copy, sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::{Element, ElementGuard};
use crate::recl::{defer, protected};
use crate::util::{
    derive_filter, get_tag, range_split, set_tag, u64_read_byte, u64_write_byte, unreachable,
    CircularRange, PtrTag,
};
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
    ($s:expr) => {{
        if $s.cells_remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
            $s.grow();
            return;
        }
    }};
}

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
            let old_group_amount = old_table.as_ref().group_amount();
            let hash_builder = old_table.as_ref().hash_builder.clone();
            let era = old_table.as_ref().era;
            let new_table = Box::into_raw(Box::new(BucketArray::new(
                root_ptr,
                old_group_amount * 16 * 2,
                era,
                hash_builder,
            )));
            let task_list = Mutex::new(range_split(0..old_group_amount, 128));

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
                defer(era, move || drop(Box::from_raw(old_table_ptr)));
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
            for group_idx in range {
                unsafe {
                    let old_table = self.old_table.as_ref();
                    let new_table = self.new_table.as_ref();
                    for (_, _, atomic_ptr) in old_table.groups[group_idx].iter() {
                        'inner: loop {
                            //let cache = atomic_cache.load();
                            let bucket_ptr = atomic_ptr.load(Ordering::SeqCst);
                            if atomic_ptr.compare_and_swap(
                                bucket_ptr,
                                set_tag(bucket_ptr, PtrTag::Resize),
                                Ordering::SeqCst,
                            ) != bucket_ptr
                            {
                                continue 'inner;
                            }
                            new_table.put_node(bucket_ptr);
                            break;
                        }
                    }
                }
            }
        }

        self.running.fetch_sub(1, Ordering::SeqCst);
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

struct CacheEntry<'a> {
    atomic: &'a AtomicU64,
    slot: usize,
}

impl<'a> CacheEntry<'a> {
    fn load(&self) -> u8 {
        u64_read_byte(self.atomic.load(Ordering::SeqCst), self.slot)
    }
}

struct Group<T> {
    cache: AtomicU64,
    nodes: [AtomicPtr<T>; 8],
}

struct Probe<'a, T> {
    i: usize,
    cache: u64,
    filter: u8,
    nodes: &'a [AtomicPtr<T>; 8],
}

impl<'a, T> Iterator for Probe<'a, T> {
    type Item = *mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i == 8 {
            return None;
        }
        if u64_read_byte(self.cache, self.i) == self.filter {
            let p = self.nodes[self.i].load(Ordering::SeqCst);
            self.i += 1;
            return Some(p);
        } else {
            self.i += 1;
            self.next()
        }
    }
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

    fn iter<'a>(&'a self) -> impl Iterator<Item = (usize, CacheEntry<'a>, &'a AtomicPtr<T>)> + 'a {
        self.nodes.iter().enumerate().map(move |(i, atomic)| {
            (
                i,
                CacheEntry {
                    atomic: &self.cache,
                    slot: i,
                },
                atomic,
            )
        })
    }

    fn probe<'a>(&'a self, filter: u8) -> Probe<'a, T> {
        Probe {
            i: 0,
            cache: self.cache.load(Ordering::SeqCst),
            filter,
            nodes: &self.nodes,
        }
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
        } else {
            return true;
        }
    }
}

type G<K, V> = Group<ABox<Element<K, V>>>;

static LOAD_FACTOR: f32 = 0.75;

fn calc_cells_remaining(capacity: usize) -> usize {
    (LOAD_FACTOR * capacity as f32) as usize
}

struct BucketArray<K, V, S> {
    root_ptr: *mut AtomicPtr<BucketArray<K, V, S>>,
    cells_remaining: AtomicUsize,
    hash_builder: Arc<S>,
    era: usize,
    next: AtomicPtr<ResizeCoordinator<K, V, S>>,
    groups: Box<[G<K, V>]>,
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
        let groups = make_groups(capacity / 8);

        Self {
            root_ptr,
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

    fn fetch_next<'a>(&self) -> Option<&'a Self> {
        let coordinator = self.next.load(Ordering::SeqCst);
        if !coordinator.is_null() {
            unsafe {
                (*coordinator).work();
                (*coordinator).wait();
                Some(&*(*coordinator).new_table.as_ptr())
            }
        } else {
            None
        }
    }

    fn grow(&self) {
        unsafe {
            let coordinator = self.next.load(Ordering::SeqCst);
            if !coordinator.is_null() {
                (*coordinator).work();
                (*coordinator).wait();
            } else {
                let new_coordinator = Box::into_raw(Box::new(ResizeCoordinator::new(
                    self.root_ptr,
                    mem::transmute(self),
                )));
                let old =
                    self.next
                        .compare_and_swap(ptr::null_mut(), new_coordinator, Ordering::SeqCst);
                if old.is_null() {
                    (*new_coordinator).run(self.era);
                } else {
                    Box::from_raw(new_coordinator);
                    (*old).work();
                    (*old).wait();
                }
            }
        }
    }

    fn find_node<Q>(&self, search_key: &Q) -> Option<*mut ABox<Element<K, V>>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if let Some(next) = self.fetch_next() {
            return next.find_node(search_key);
        }
        let hash = do_hash(&*self.hash_builder, search_key);
        let group_idx_start = hash as usize % self.group_amount();
        let filter = derive_filter(hash);
        for group_idx in CircularRange::new(0, self.group_amount(), group_idx_start) {
            let group = &self.groups[group_idx];
            'm: for bucket_ptr in group.probe(filter) {
                match get_tag(bucket_ptr) {
                    PtrTag::Resize => {
                        return self.fetch_next().unwrap().find_node(search_key);
                    }
                    PtrTag::Tombstone => continue 'm,
                    PtrTag::None => (),
                }
                if bucket_ptr.is_null() {
                    return None;
                }
                let bucket_data = sarc_deref(bucket_ptr);
                if search_key == bucket_data.key.borrow() {
                    return Some(bucket_ptr as _);
                } else {
                    continue 'm;
                }
            }
        }
        unreachable()
    }

    fn remove<Q>(&self, search_key: &Q) -> Option<*mut ABox<Element<K, V>>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if let Some(next) = self.fetch_next() {
            return next.remove(search_key);
        }
        let hash = do_hash(&*self.hash_builder, search_key);
        let group_idx_start = hash as usize % self.group_amount();
        for group_idx in CircularRange::new(0, self.group_amount(), group_idx_start) {
            let group = &self.groups[group_idx];
            'm: for (i, cache, bucket_ptr) in group.iter() {
                'inner: loop {
                    let bucket_ptr = bucket_ptr.load(Ordering::SeqCst);
                    let cache = cache.load();
                    match get_tag(bucket_ptr) {
                        PtrTag::Resize => {
                            return self.fetch_next().unwrap().remove(search_key);
                        }
                        PtrTag::Tombstone => continue 'm,
                        PtrTag::None => (),
                    }
                    if bucket_ptr.is_null() {
                        return None;
                    } else if search_key == sarc_deref(bucket_ptr).key.borrow() {
                        let tombstone = set_tag(ptr::null_mut(), PtrTag::Tombstone);
                        if group.try_publish(i, cache, bucket_ptr, 0, tombstone) {
                            defer(self.era, move || sarc_remove_copy(bucket_ptr));
                            return Some(bucket_ptr);
                        } else {
                            continue 'inner;
                        }
                    }
                }
            }
        }

        unreachable();
    }

    fn put_node(&self, node: *mut ABox<Element<K, V>>) {
        if let Some(next) = self.fetch_next() {
            next.put_node(node);
            return;
        }
        let node_data = sarc_deref(node);
        let hash = do_hash(&*self.hash_builder, &node_data.key);
        let group_idx_start = hash as usize % self.group_amount();
        let filter = derive_filter(hash);
        for group_idx in CircularRange::new(0, self.group_amount(), group_idx_start) {
            let group = &self.groups[group_idx];
            for (i, cache, bucket_ptr) in group.iter() {
                'inner: loop {
                    let bucket_ptr = bucket_ptr.load(Ordering::SeqCst);
                    let cache = cache.load();
                    match get_tag(bucket_ptr) {
                        PtrTag::None => (),
                        PtrTag::Resize => {
                            return self.fetch_next().unwrap().put_node(node);
                        }
                        PtrTag::Tombstone => {
                            if group.try_publish(i, cache, bucket_ptr, filter, node) {
                                // Don't update cells_remaining since we are replacing a tombstone.
                                return;
                            } else {
                                continue 'inner;
                            }
                        }
                    }
                    if bucket_ptr.is_null() {
                        if group.try_publish(i, cache, bucket_ptr, filter, node) {
                            maybe_grow!(self);
                            return;
                        } else {
                            continue 'inner;
                        }
                    } else {
                        let bucket_data = sarc_deref(bucket_ptr);
                        if bucket_data.key == node_data.key {
                            if group.try_publish(i, cache, bucket_ptr, filter, node) {
                                // Don't update cells_remaining since we are replacing an entry.
                                return;
                            } else {
                                continue 'inner;
                            }
                        } else {
                            break 'inner;
                        }
                    }
                }
            }
        }
    }
}

pub struct Table<K, V, S> {
    hash_builder: Arc<S>,
    array: Box<AtomicPtr<BucketArray<K, V, S>>>,
}

impl<K: Eq + Hash, V, S: BuildHasher> Table<K, V, S> {
    pub fn new(capacity: usize, era: usize, hash_builder: Arc<S>) -> Self {
        let mut atomic = Box::new(AtomicPtr::new(ptr::null_mut()));
        let table = BucketArray::new(&mut *atomic, capacity, era, Arc::clone(&hash_builder));
        atomic.store(Box::into_raw(Box::new(table)), Ordering::SeqCst);

        Self {
            hash_builder,
            array: atomic,
        }
    }

    fn array<'a>(&self) -> &'a BucketArray<K, V, S> {
        unsafe { &*self.array.load(Ordering::SeqCst) }
    }

    pub fn insert(&self, key: K, value: V) {
        let hash = do_hash(&*self.hash_builder, &key);
        let node = sarc_new(Element::new(key, hash, value));
        protected(|| self.array().put_node(node));
    }

    pub fn get<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.array().find_node(search_key).map(Element::read))
    }

    pub fn remove<Q>(&self, search_key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.array().remove(search_key).is_some())
    }

    pub fn remove_take<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.array().remove(search_key).map(Element::read))
    }
}

#[cfg(test)]
mod tests {
    use super::Table;
    use std::collections::hash_map::RandomState;
    use std::sync::Arc;

    #[test]
    fn insert_get() {
        let table = Table::new(4, 1, Arc::new(RandomState::new()));
        table.insert(4i32, 9i32);
        table.insert(8i32, 24i32);
        assert_eq!(*table.get(&4).unwrap(), 9);
        assert_eq!(*table.get(&8).unwrap(), 24);
    }

    #[test]
    fn insert_remove() {
        let table = Table::new(4, 1, Arc::new(RandomState::new()));
        table.insert(4i32, 9i32);
        table.insert(8i32, 24i32);
        assert_eq!(*table.remove_take(&4).unwrap(), 9);
        assert_eq!(*table.remove_take(&8).unwrap(), 24);
        assert!(!table.remove(&40));
    }
}
