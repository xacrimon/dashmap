mod recl;

use crate::alloc::{sarc_add_copy, sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::{Element, ElementGuard};
use crate::table::Table as TableTrait;
use crate::util::{
    derive_filter, get_cache, get_tag_type, range_split, set_cache, set_tag_type, tag_strip,
    unreachable, CircularRange, FastCounter, PtrTag,
};
use recl::{defer, protected, enter_critical, exit_critical};
use std::borrow::Borrow;
use std::cmp;
use std::collections::LinkedList;
use std::hash::{BuildHasher, Hash, Hasher};
use std::mem;
use std::ops::Range;
use std::ptr::{self, NonNull};
use std::sync::atomic::{spin_loop_hint, AtomicPtr, AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

macro_rules! maybe_grow {
    ($s:expr) => {
        if $s.cells_remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
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
    ($ptr:expr) => {{
        defer(move || reap_now!($ptr));
    }};
}

fn do_hash(f: &impl BuildHasher, i: &(impl ?Sized + Hash)) -> u64 {
    let mut hasher = f.build_hasher();
    i.hash(&mut hasher);
    hasher.finish()
}

fn make_groups<K, V>(amount: usize) -> Box<[AtomicPtr<Bucket<K, V>>]> {
    (0..amount)
        .map(|_| AtomicPtr::new(ptr::null_mut()))
        .collect()
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
            let new_table = on_heap!(BucketArray::new(
                root_ptr,
                old_slot_amount * 2,
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

    fn run(&self) {
        self.work();
        self.wait();
        unsafe {
            if (*self.root_ptr).compare_and_swap(
                self.old_table.as_ptr(),
                self.new_table.as_ptr(),
                Ordering::AcqRel,
            ) == self.old_table.as_ptr()
            {
                let old_table_ptr = self.old_table.as_ptr();
                reap_defer!(old_table_ptr);
            }
        }
    }

    fn wait(&self) {
        while self.running.load(Ordering::Acquire) != 0 {
            spin_loop_hint()
        }
    }

    fn work(&self) {
        self.running.fetch_add(1, Ordering::AcqRel);
        while let Some(range) = self.task_list.lock().unwrap().pop_front() {
            for idx in range {
                unsafe {
                    'inner: loop {
                        let bucket_ptr =
                            self.old_table.as_ref().buckets[idx].load(Ordering::Relaxed);
                        if !bucket_ptr.is_null() && get_tag_type(bucket_ptr as _) == PtrTag::None {
                            if self.old_table.as_ref().buckets[idx].compare_and_swap(
                                bucket_ptr,
                                set_tag_type(bucket_ptr as usize, PtrTag::Resize) as _,
                                Ordering::AcqRel,
                            ) != bucket_ptr
                            {
                                continue 'inner;
                            }
                            let cs = tag_strip(bucket_ptr as usize) as *mut ABox<Element<K, V>>;
                            sarc_add_copy(cs);
                            self.new_table.as_ref().put_node(cs);
                        }
                        break;
                    }
                }
            }
        }

        self.running.fetch_sub(1, Ordering::AcqRel);
    }
}

fn calc_cells_remaining(capacity: usize) -> usize {
    (LOAD_FACTOR * capacity as f32) as usize
}

fn calc_capacity(capacity: usize) -> usize {
    let factor = 1.0 / LOAD_FACTOR;
    (capacity as f32 * factor) as usize
}

pub struct BucketArrayIter<K, V> {
    buckets: *const [AtomicPtr<Bucket<K, V>>],
    next: usize,
}

impl<K: Eq + Hash, V> Iterator for BucketArrayIter<K, V> {
    type Item = ElementGuard<K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.next >= (&*self.buckets).len() {
                return None;
            }

            let bucket_ptr = (&*self.buckets).get_unchecked(self.next).load(Ordering::Relaxed);
            let data_ptr = tag_strip(bucket_ptr as _) as *mut ABox<Element<K, V>>;
            self.next += 1;

            if !data_ptr.is_null() {
                return Some(Element::read(data_ptr));
            } else {
                return self.next();
            }
        }
    }
}

impl<K, V> Drop for BucketArrayIter<K, V> {
    fn drop(&mut self) {
        exit_critical();
    }
}

struct BucketArray<K, V, S> {
    root_ptr: *mut AtomicPtr<BucketArray<K, V, S>>,
    cells_remaining: AtomicUsize,
    hash_builder: Arc<S>,
    next: AtomicPtr<ResizeCoordinator<K, V, S>>,
    buckets: Box<[AtomicPtr<Bucket<K, V>>]>,
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn iter(&self) -> BucketArrayIter<K, V> {
        enter_critical();

        BucketArrayIter {
            buckets: &*self.buckets,
            next: 0,
        }
    }

    fn new(
        root_ptr: *mut AtomicPtr<BucketArray<K, V, S>>,
        capacity: usize,
        hash_builder: Arc<S>,
    ) -> Self {
        let capacity = calc_capacity(cmp::max(capacity, 16)).next_power_of_two();
        let cells_remaining = calc_cells_remaining(capacity);
        let buckets = make_groups(capacity);

        Self {
            root_ptr,
            cells_remaining: AtomicUsize::new(cells_remaining),
            hash_builder,
            next: AtomicPtr::new(ptr::null_mut()),
            buckets,
        }
    }

    fn fetch_next<'a>(&self) -> Option<&'a Self> {
        let coordinator = self.next.load(Ordering::Relaxed);
        if coordinator.is_null() {
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
            let coordinator = self.next.load(Ordering::Acquire);
            if !coordinator.is_null() {
                (*coordinator).work();
                (*coordinator).wait();
            } else {
                let new_coordinator =
                    on_heap!(ResizeCoordinator::new(self.root_ptr, mem::transmute(self),));
                let old =
                    self.next
                        .compare_and_swap(ptr::null_mut(), new_coordinator, Ordering::AcqRel);
                if old.is_null() {
                    (*new_coordinator).run();
                    reap_now!(new_coordinator);
                } else {
                    reap_now!(new_coordinator);
                    (*old).work();
                    (*old).wait();
                }
            }
        }
    }

    fn optimistic_update<Q, F>(
        &self,
        search_key: &Q,
        do_update: &mut F,
    ) -> Option<*mut ABox<Element<K, V>>>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        let maybe_next = self.fetch_next();
        if maybe_next.is_some() {
            let next: &BucketArray<K, V, S> = unsafe { mem::transmute(maybe_next) };
            return next.optimistic_update(search_key, do_update);
        }
        let hash = do_hash(&*self.hash_builder, search_key);
        let filter = derive_filter(hash);
        let idx_start = hash as usize % self.buckets.len();
        for idx in CircularRange::new(0, self.buckets.len(), idx_start) {
            'inner: loop {
                let bucket_ptr = self.buckets[idx].load(Ordering::Relaxed);
                let cache = get_cache(bucket_ptr as _);
                if bucket_ptr.is_null() {
                    return None;
                }
                if filter == cache {
                    match get_tag_type(bucket_ptr as _) {
                        PtrTag::Resize => {
                            return self
                                .fetch_next()
                                .unwrap()
                                .optimistic_update(search_key, do_update);
                        }
                        PtrTag::Tombstone => break 'inner,
                        PtrTag::None => (),
                    }
                    let stripped = tag_strip(bucket_ptr as _) as *mut ABox<Element<K, V>>;
                    let bucket_data = sarc_deref(stripped);
                    if search_key == bucket_data.key.borrow() {
                        let updated_value = do_update(&bucket_data.key, &bucket_data.value);
                        let new_bucket_uc = sarc_new(Element::new(
                            bucket_data.key.clone(),
                            bucket_data.hash,
                            updated_value,
                        ));
                        let new_bucket = set_cache(new_bucket_uc as _, cache);
                        if self.buckets[idx].compare_and_swap(
                            bucket_ptr,
                            new_bucket as _,
                            Ordering::AcqRel,
                        ) == bucket_ptr
                        {
                            defer(move || sarc_remove_copy(stripped));
                            return Some(new_bucket_uc);
                        } else {
                            sarc_remove_copy(new_bucket_uc);
                            continue 'inner;
                        }
                    } else {
                        break;
                    }
                } else {
                    break 'inner;
                }
            }
        }
        unreachable();
    }

    fn remove_if<Q>(
        &self,
        search_key: &Q,
        predicate: &mut impl FnMut(&K, &V) -> bool,
    ) -> Option<*mut ABox<Element<K, V>>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if let Some(next) = self.fetch_next() {
            return next.remove_if(search_key, predicate);
        }
        let hash = do_hash(&*self.hash_builder, search_key);
        let filter = derive_filter(hash);
        let idx_start = hash as usize % self.buckets.len();
        for idx in CircularRange::new(0, self.buckets.len(), idx_start) {
            'inner: loop {
                let bucket_ptr = self.buckets[idx].load(Ordering::Relaxed);
                let cache = get_cache(bucket_ptr as _);
                if bucket_ptr.is_null() {
                    return None;
                }
                if filter == cache {
                    match get_tag_type(bucket_ptr as _) {
                        PtrTag::Resize => {
                            return self.fetch_next().unwrap().remove_if(search_key, predicate);
                        }
                        PtrTag::Tombstone => break 'inner,
                        PtrTag::None => (),
                    }
                    if {
                        let cs = tag_strip(bucket_ptr as _);
                        let bucket_data = sarc_deref(cs as *mut ABox<Element<K, V>>);
                        search_key == bucket_data.key.borrow()
                            && predicate(&bucket_data.key, &bucket_data.value)
                    } {
                        let tombstone = set_tag_type(0, PtrTag::Tombstone);
                        if self.buckets[idx].compare_and_swap(
                            bucket_ptr,
                            tombstone as _,
                            Ordering::AcqRel,
                        ) == bucket_ptr
                        {
                            let stripped = tag_strip(bucket_ptr as _) as *mut ABox<Element<K, V>>;
                            defer(move || sarc_remove_copy(stripped));
                            return Some(stripped);
                        } else {
                            continue 'inner;
                        }
                    } else {
                        break 'inner;
                    }
                } else {
                    break 'inner;
                }
            }
        }
        unreachable();
    }

    fn find_node<Q>(&self, search_key: &Q) -> Option<*mut ABox<Element<K, V>>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let maybe_next = self.fetch_next();
        if maybe_next.is_some() {
            let next: &BucketArray<K, V, S> = unsafe { mem::transmute(maybe_next) };
            return next.find_node(search_key);
        }
        let hash = do_hash(&*self.hash_builder, search_key);
        let idx_start = hash as usize & (self.buckets.len() - 1);
        let filter = derive_filter(hash);
        for idx in CircularRange::new(0, self.buckets.len(), idx_start) {
            let bucket_ptr = unsafe { self.buckets.get_unchecked(idx).load(Ordering::Relaxed) };
            if bucket_ptr.is_null() {
                return None;
            }
            if filter == get_cache(bucket_ptr as _) {
                match get_tag_type(bucket_ptr as _) {
                    PtrTag::Resize => {
                        return self.fetch_next().unwrap().find_node(search_key);
                    }
                    PtrTag::Tombstone => {
                        continue;
                    }
                    PtrTag::None => (),
                }
                let cs = tag_strip(bucket_ptr as _) as *mut ABox<Element<K, V>>;
                let bucket_data = sarc_deref(cs);
                if search_key == bucket_data.key.borrow() {
                    return Some(cs as _);
                } else {
                    continue;
                }
            }
        }
        unreachable();
    }

    fn put_node(&self, mut node: *mut ABox<Element<K, V>>) -> Option<*mut ABox<Element<K, V>>> {
        if let Some(next) = self.fetch_next() {
            return next.put_node(node);
        }
        let node_data = sarc_deref(node);
        let hash = do_hash(&*self.hash_builder, &node_data.key);
        let idx_start = hash as usize % self.buckets.len();
        let filter = derive_filter(hash);
        node = set_tag_type(node as _, PtrTag::None) as _;
        node = set_cache(node as _, filter) as _;
        for idx in CircularRange::new(0, self.buckets.len(), idx_start) {
            'inner: loop {
                let bucket_ptr = self.buckets[idx].load(Ordering::Relaxed);
                let cache = get_cache(bucket_ptr as _);
                match get_tag_type(bucket_ptr as _) {
                    PtrTag::None => (),
                    PtrTag::Resize => {
                        return self.fetch_next().unwrap().put_node(node);
                    }
                    PtrTag::Tombstone => {
                        if self.buckets[idx].compare_and_swap(bucket_ptr, node, Ordering::AcqRel)
                            == bucket_ptr
                        {
                            // Don't update cells_remaining since we are replacing a tombstone.
                            return None;
                        } else {
                            continue 'inner;
                        }
                    }
                }
                if bucket_ptr.is_null() {
                    if self.buckets[idx].compare_and_swap(bucket_ptr, node, Ordering::AcqRel)
                        == bucket_ptr
                    {
                        maybe_grow!(self);
                        return None;
                    } else {
                        continue 'inner;
                    }
                } else {
                    if filter != cache {
                        break 'inner;
                    }
                    let stripped = tag_strip(bucket_ptr as _) as *mut ABox<Element<K, V>>;
                    let bucket_data = sarc_deref(stripped);
                    if bucket_data.key == node_data.key {
                        if self.buckets[idx].compare_and_swap(bucket_ptr, node, Ordering::AcqRel)
                            == bucket_ptr
                        {
                            // Don't update cells_remaining since we are replacing an entry.
                            return Some(stripped);
                        } else {
                            continue 'inner;
                        }
                    } else {
                        break 'inner;
                    }
                }
            }
        }
        unreachable();
    }

    fn retain(&self, predicate: &mut impl FnMut(&K, &V) -> bool) {
        if let Some(next) = self.fetch_next() {
            return next.retain(predicate);
        }
        for idx in 0..self.buckets.len() {
            'inner: loop {
                let bucket_ptr = self.buckets[idx].load(Ordering::Relaxed);
                let _cache = get_cache(bucket_ptr as _);
                match get_tag_type(bucket_ptr as _) {
                    PtrTag::Resize => return self.fetch_next().unwrap().retain(predicate),
                    PtrTag::Tombstone => break 'inner,
                    PtrTag::None => (),
                }
                if bucket_ptr.is_null() {
                    break 'inner;
                }
                let cs = tag_strip(bucket_ptr as _) as *mut ABox<Element<K, V>>;
                let bucket_data = sarc_deref(cs);
                if !predicate(&bucket_data.key, &bucket_data.value) {
                    let tombstone = set_tag_type(0, PtrTag::Tombstone);
                    if self.buckets[idx].compare_and_swap(
                        bucket_ptr,
                        tombstone as _,
                        Ordering::AcqRel,
                    ) == bucket_ptr
                    {
                        defer(move || sarc_remove_copy(cs));
                        break 'inner;
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

impl<K, V, S> Drop for BucketArray<K, V, S> {
    fn drop(&mut self) {
        for idx in 0..self.buckets.len() {
            let bucket_ptr = self.buckets[idx].load(Ordering::Acquire);
            let stripped = tag_strip(bucket_ptr as _) as *mut ABox<Element<K, V>>;
            if !stripped.is_null() {
                sarc_remove_copy(stripped);
            }
        }
    }
}

pub struct Table<K, V, S> {
    hash_builder: Arc<S>,
    len: FastCounter,
    array: Box<AtomicPtr<BucketArray<K, V, S>>>,
}

impl<K, V, S> Drop for Table<K, V, S> {
    fn drop(&mut self) {
        unsafe {
            let array = self.array.load(Ordering::Acquire);
            if !array.is_null() {
                ptr::drop_in_place(array);
            }
        }
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for Table<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for Table<K, V, S> {}

impl<K: Eq + Hash, V, S: BuildHasher> Table<K, V, S> {
    fn array<'a>(&self) -> &'a BucketArray<K, V, S> {
        unsafe { &*self.array.load(Ordering::Relaxed) }
    }
}

impl<K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> TableTrait<K, V, S>
    for Table<K, V, S>
{
    type Iter = BucketArrayIter<K, V>;

    fn iter(&self) -> Self::Iter {
        protected(|| self.array().iter())
    }

    fn new(capacity: usize, hash_builder: S) -> Self {
        let hash_builder = Arc::new(hash_builder);
        let mut atomic = Box::new(AtomicPtr::new(ptr::null_mut()));
        let table = BucketArray::new(&mut *atomic, capacity, Arc::clone(&hash_builder));
        atomic.store(on_heap!(table), Ordering::Release);

        Self {
            hash_builder,
            len: FastCounter::new(),
            array: atomic,
        }
    }

    fn insert(&self, key: K, value: V) -> bool {
        let hash = do_hash(&*self.hash_builder, &key);
        let node = sarc_new(Element::new(key, hash, value));
        if protected(|| self.array().put_node(node)).is_none() {
            self.len.increment();
            false
        } else {
            true
        }
    }

    fn insert_and_get(&self, key: K, value: V) -> ElementGuard<K, V> {
        let hash = do_hash(&*self.hash_builder, &key);
        let node = sarc_new(Element::new(key, hash, value));
        let g = Element::read(node);
        if protected(|| self.array().put_node(node)).is_none() {
            self.len.increment();
            g
        } else {
            g
        }
    }

    fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>> {
        println!("sc1");
        let hash = do_hash(&*self.hash_builder, &key);
        println!("sc2");
        let node = sarc_new(Element::new(key, hash, value));

        protected(|| {
            println!("sc3");
            if let Some(old_ptr) = self.array().put_node(node) {
                println!("sc4 {:x}", old_ptr as usize);
                let guard = Element::read(old_ptr);
                println!("sc5");
                return Some(guard);
            } else {
                println!("sc6");
                self.len.increment();
                return None;
            }
        })
    }

    fn get<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.array().find_node(search_key).map(Element::read))
    }

    fn contains_key<Q>(&self, search_key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.array().find_node(search_key)).is_some()
    }

    fn remove<Q>(&self, search_key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if protected(|| self.array().remove_if(search_key, &mut |_, _| true)).is_some() {
            self.len.decrement();
            true
        } else {
            false
        }
    }

    fn remove_if<Q>(&self, search_key: &Q, predicate: &mut impl FnMut(&K, &V) -> bool) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if protected(|| self.array().remove_if(search_key, predicate)).is_some() {
            self.len.decrement();
            true
        } else {
            false
        }
    }

    fn remove_take<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if let Some(r) = protected(|| {
            self.array()
                .remove_if(search_key, &mut |_, _| true)
                .map(Element::read)
        }) {
            self.len.decrement();
            Some(r)
        } else {
            None
        }
    }

    fn remove_if_take<Q>(
        &self,
        search_key: &Q,
        predicate: &mut impl FnMut(&K, &V) -> bool,
    ) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if let Some(r) = protected(|| {
            self.array()
                .remove_if(search_key, predicate)
                .map(Element::read)
        }) {
            self.len.decrement();
            Some(r)
        } else {
            None
        }
    }

    fn extract<T, Q, F>(&self, search_key: &Q, do_extract: F) -> Option<T>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnOnce(&K, &V) -> T,
    {
        protected(|| {
            self.array().find_node(search_key).map(|ptr| {
                let elem = sarc_deref(ptr);
                do_extract(&elem.key, &elem.value)
            })
        })
    }

    fn update<Q, F>(&self, search_key: &Q, do_update: &mut F) -> bool
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        protected(|| self.array().optimistic_update(search_key, do_update)).is_some()
    }

    fn update_get<Q, F>(&self, search_key: &Q, do_update: &mut F) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        protected(|| {
            self.array()
                .optimistic_update(search_key, do_update)
                .map(Element::read)
        })
    }

    fn retain(&self, predicate: &mut impl FnMut(&K, &V) -> bool) {
        protected(|| self.array().retain(predicate));
    }

    fn clear(&self) {
        self.retain(&mut |_, _| false);
    }

    fn len(&self) -> usize {
        self.len.read()
    }

    fn capacity(&self) -> usize {
        protected(|| self.array().buckets.len() as f32 * LOAD_FACTOR) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::Table;
    use crate::table::Table as TableTrait;
    use std::collections::hash_map::RandomState;

    #[test]
    fn insert_replace() {
        let map = Table::new(16, RandomState::new());
        map.insert("I am the key!", "I'm the old value!");
        let maybe_old_entry = map.replace("I am the key!", "And I am the value!");
        assert!(maybe_old_entry.is_some());
        let old_entry = maybe_old_entry.unwrap();
        assert!(*old_entry.value() == "I'm the old value!");
    }

    #[test]
    fn insert_get() {
        let table = Table::new(16, RandomState::new());
        table.insert(4i32, 9i32);
        table.insert(8i32, 24i32);
        assert_eq!(*table.get(&4).unwrap(), 9);
        assert_eq!(*table.get(&8).unwrap(), 24);
    }

    #[test]
    fn insert_remove() {
        let table = Table::new(16, RandomState::new());
        table.insert(4i32, 9i32);
        table.insert(8i32, 24i32);
        assert_eq!(*table.remove_take(&4).unwrap(), 9);
        assert_eq!(*table.remove_take(&8).unwrap(), 24);
        assert!(!table.remove(&40));
    }

    #[test]
    fn insert_len() {
        let table = Table::new(16, RandomState::new());
        table.insert(4i32, 9i32);
        table.insert(8i32, 24i32);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn insert_update_get() {
        let table = Table::new(16, RandomState::new());
        table.insert(8i32, 24i32);
        table.update(&8, &mut |_, v| v * 2);
        assert_eq!(*table.get(&8).unwrap(), 48);
    }

    #[test]
    fn insert_update_get_fused() {
        let table = Table::new(16, RandomState::new());
        table.insert(8i32, 24i32);
        assert_eq!(*table.update_get(&8, &mut |_, v| v * 2).unwrap(), 48);
    }
}
