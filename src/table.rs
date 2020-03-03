#![allow(clippy::cast_ptr_alignment)]

use crate::alloc::{sarc_add_copy, sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::*;
use crate::recl::{defer, protected};
use crate::util::{p_set_tag, p_tag, CachePadded};
use std::alloc::{alloc, dealloc, Layout};
use std::borrow::Borrow;
use std::cmp;
use std::hash::{BuildHasher, Hash, Hasher};
use std::mem;
use std::ptr;
use std::slice;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::Arc;

const REDIRECT_TAG: usize = 5;
const TOMBSTONE_TAG: usize = 7;

pub fn hash2idx(hash: u64, len: usize) -> usize {
    hash as usize % len
}

pub fn do_hash(f: &impl BuildHasher, i: &(impl ?Sized + Hash)) -> u64 {
    let mut hasher = f.build_hasher();
    i.hash(&mut hasher);
    hasher.finish()
}

macro_rules! cell_maybe_return {
    ($s:expr, $b:expr) => {{
        let should_grow = $s.remaining_cells.fetch_sub(1, Ordering::SeqCst) == 1;
        if should_grow {
            return ($s.grow(), $b);
        } else {
            return (None, $b);
        }
    }};
}

macro_rules! cell_maybe_return_k3 {
    ($s:expr) => {{
        let should_grow = $s.remaining_cells.fetch_sub(1, Ordering::SeqCst) == 1;
        if should_grow {
            return $s.grow();
        } else {
            return None;
        }
    }};
}

fn incr_idx<K, V, S>(s: &BucketArray<K, V, S>, i: usize) -> usize {
    hash2idx(i as u64 + 1, s.capacity)
}

enum InsertResult {
    None,
    Grow,
}

impl<K, V, S> Drop for BucketArray<K, V, S> {
    fn drop(&mut self) {
        let mut garbage = Vec::with_capacity(self.capacity);
        let buckets = self.buckets();
        for bucket in &*buckets {
            let ptr = p_set_tag(bucket.load(Ordering::SeqCst), 0);
            if !ptr.is_null() {
                garbage.push(ptr);
            }
        }
        defer(self.era, move || {
            for ptr in garbage {
                sarc_remove_copy(ptr);
            }
        });
    }
}

pub struct BucketArray<K, V, S> {
    remaining_cells: CachePadded<AtomicUsize>,
    era: usize,
    next: AtomicPtr<Self>,
    hash_builder: Arc<S>,
    capacity: usize,
}

impl<K: Eq + Hash + Clone, V, S: BuildHasher> BucketArray<K, V, S> {
    fn optimistic_update<Q, F>(&self, search_key: &Q, do_update: &mut F) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        let buckets = self.buckets();
        let hash = do_hash(&*self.hash_builder, search_key);
        let mut idx = hash2idx(hash, self.capacity);

        loop {
            let bucket_ptr = unsafe { buckets.get_unchecked(idx).load(Ordering::SeqCst) };
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    if self
                        .get_next()
                        .unwrap()
                        .optimistic_update(search_key, do_update)
                    {
                        return true;
                    } else {
                        idx = incr_idx(self, idx);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    idx = incr_idx(self, idx);
                    continue;
                }

                _ => (),
            }

            if bucket_ptr.is_null() {
                return false;
            }

            let bucket_data = sarc_deref(bucket_ptr);
            if search_key == bucket_data.key.borrow() {
                let new_value = do_update(&bucket_data.key, &bucket_data.value);
                let new_element =
                    Element::new(bucket_data.key.clone(), bucket_data.hash, new_value);
                let new_ptr = sarc_new(new_element);

                if unsafe { buckets.get_unchecked(idx) }.compare_and_swap(
                    bucket_ptr,
                    new_ptr,
                    Ordering::SeqCst,
                ) == bucket_ptr
                {
                    defer(self.era, move || sarc_remove_copy(bucket_ptr));
                    return true;
                } else {
                    sarc_remove_copy(new_ptr);
                    continue;
                }
            } else {
                idx = incr_idx(self, idx);
            }
        }
    }
}

fn ba_layout<K, V, S>(bucket_amount: usize) -> Layout {
    let start_size = mem::size_of::<BucketArray<K, V, S>>();
    let align = mem::align_of::<BucketArray<K, V, S>>();
    let total_size = start_size + bucket_amount * mem::size_of::<usize>();
    unsafe { Layout::from_size_align_unchecked(total_size, align) }
}

fn ba_drop<K, V, S>(ba: *mut BucketArray<K, V, S>) {
    unsafe {
        let capacity = (*ba).capacity;
        let layout = ba_layout::<K, V, S>(capacity);
        ptr::drop_in_place(ba);
        dealloc(ba as _, layout);
    }
}

impl<K, V, S> BucketArray<K, V, S> {
    fn buckets<'a>(&'a self) -> &'a [AtomicPtr<ABox<Element<K, V>>>] {
        let self_begin = self as *const _ as usize as *mut u8;
        unsafe {
            let array_begin = self_begin.add(mem::size_of::<Self>());
            slice::from_raw_parts(array_begin as _, self.capacity)
        }
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(mut capacity: usize, hash_builder: Arc<S>, era: usize) -> *mut Self {
        unsafe {
            capacity = cmp::max(2 * capacity, 16);
            let remaining_cells = CachePadded::new(AtomicUsize::new(capacity * 3 / 4));
            let layout = ba_layout::<K, V, S>(capacity);
            let p = alloc(layout);
            let s = Self {
                remaining_cells,
                hash_builder,
                capacity,
                era,
                next: AtomicPtr::new(0 as _),
            };
            ptr::write(p as _, s);
            let array_start = p.add(mem::size_of::<Self>());
            for i in 0..capacity {
                let p2p = (array_start as *mut AtomicPtr<ABox<Element<K, V>>>).add(i);
                *p2p = AtomicPtr::new(ptr::null_mut::<ABox<Element<K, V>>>());
            }
            p as _
        }
    }

    fn extract<T, Q, F>(&self, search_key: &Q, do_extract: F) -> Option<T>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnOnce(&K, &V) -> T,
    {
        self.get_elem(search_key).map(|ptr| {
            let elem = sarc_deref(ptr);
            do_extract(&elem.key, &elem.value)
        })
    }

    fn insert_node<'a>(&self, node: *mut ABox<Element<K, V>>) -> (Option<*const Self>, bool) {
        let buckets = self.buckets();

        if let Some(next) = self.get_next() {
            return next.insert_node(node);
        }

        let node_data = sarc_deref(node);
        let mut idx = hash2idx(node_data.hash, self.capacity);

        loop {
            let current_bucket_ptr = unsafe { buckets.get_unchecked(idx).load(Ordering::SeqCst) };
            match p_tag(current_bucket_ptr) {
                REDIRECT_TAG => {
                    return self.get_next().unwrap().insert_node(node);
                }

                TOMBSTONE_TAG => {
                    if unsafe { buckets.get_unchecked(idx) }.compare_and_swap(
                        current_bucket_ptr,
                        node,
                        Ordering::SeqCst,
                    ) == current_bucket_ptr
                    {
                        cell_maybe_return!(self, false);
                    } else {
                        continue;
                    }
                }

                _ => (),
            }

            if !current_bucket_ptr.is_null() {
                let current_bucket_data = sarc_deref(current_bucket_ptr);
                if current_bucket_data.key == node_data.key {
                    if unsafe { buckets.get_unchecked(idx) }.compare_and_swap(
                        current_bucket_ptr,
                        node,
                        Ordering::SeqCst,
                    ) == current_bucket_ptr
                    {
                        defer(self.era, move || sarc_remove_copy(current_bucket_ptr));
                        cell_maybe_return!(self, true);
                    } else {
                        continue;
                    }
                } else {
                    idx = incr_idx(self, idx);
                    continue;
                }
            } else {
                if unsafe { buckets.get_unchecked(idx) }.compare_and_swap(
                    current_bucket_ptr,
                    node,
                    Ordering::SeqCst,
                ) == current_bucket_ptr
                {
                    cell_maybe_return!(self, false);
                } else {
                    continue;
                }
            }
        }
    }

    fn insert_g_grow<'a>(&self, node: *mut ABox<Element<K, V>>) -> Option<*const Self> {
        let buckets = self.buckets();

        if let Some(next) = self.get_next() {
            return next.insert_g_grow(node);
        }

        let node_data = sarc_deref(node);
        let mut idx = hash2idx(node_data.hash, self.capacity);

        loop {
            let current_bucket_ptr = unsafe { buckets.get_unchecked(idx).load(Ordering::SeqCst) };
            match p_tag(current_bucket_ptr) {
                REDIRECT_TAG => {
                    return None;
                }

                TOMBSTONE_TAG => {
                    if unsafe { buckets.get_unchecked(idx) }.compare_and_swap(
                        current_bucket_ptr,
                        node,
                        Ordering::SeqCst,
                    ) == current_bucket_ptr
                    {
                        cell_maybe_return_k3!(self);
                    } else {
                        continue;
                    }
                }

                _ => (),
            }

            if !current_bucket_ptr.is_null() {
                let current_bucket_data = sarc_deref(current_bucket_ptr);
                if current_bucket_data.key == node_data.key {
                    return None;
                } else {
                    idx = incr_idx(self, idx);
                    continue;
                }
            } else {
                if unsafe { buckets.get_unchecked(idx) }.compare_and_swap(
                    current_bucket_ptr,
                    node,
                    Ordering::SeqCst,
                ) == current_bucket_ptr
                {
                    cell_maybe_return_k3!(self);
                } else {
                    continue;
                }
            }
        }
    }

    fn grow<'a>(&self) -> Option<*const Self> {
        if !self.next.load(Ordering::SeqCst).is_null() {
            return None;
        }
        let new_table = BucketArray::new(self.capacity * 2, self.hash_builder.clone(), self.era);
        if !self
            .next
            .compare_and_swap(ptr::null_mut(), new_table, Ordering::SeqCst)
            .is_null()
        {
            return None;
        }
        let new_table_ref = unsafe { &mut *new_table };
        for atomic_bucket in self.buckets() {
            loop {
                let bucket_ptr = atomic_bucket.load(Ordering::SeqCst);
                let redirect_variant = p_set_tag(bucket_ptr, REDIRECT_TAG);
                if atomic_bucket.compare_and_swap(bucket_ptr, redirect_variant, Ordering::SeqCst)
                    != bucket_ptr
                {
                    continue;
                }
                if !bucket_ptr.is_null() {
                    sarc_add_copy(bucket_ptr);
                    new_table_ref.insert_g_grow(bucket_ptr);
                }
                break;
            }
        }
        Some(new_table as _)
    }

    fn get_next<'a>(&self) -> Option<&'a Self> {
        unsafe {
            let p = self.next.load(Ordering::SeqCst);
            if !p.is_null() {
                Some(&*p)
            } else {
                None
            }
        }
    }

    fn remove<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let buckets = self.buckets();
        let hash = do_hash(&*self.hash_builder, key);
        let mut idx = hash2idx(hash, self.capacity);

        loop {
            let bucket_ptr = unsafe { buckets.get_unchecked(idx).load(Ordering::SeqCst) };
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    if self.get_next().unwrap().remove(key) {
                        return true;
                    } else {
                        idx = incr_idx(self, idx);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    idx = incr_idx(self, idx);
                    continue;
                }

                _ => (),
            }
            if bucket_ptr.is_null() {
                return false;
            }
            let bucket_data = sarc_deref(bucket_ptr);
            if key == bucket_data.key.borrow() {
                let tombstone = p_set_tag(ptr::null_mut(), TOMBSTONE_TAG);
                if unsafe { buckets.get_unchecked(idx) }.compare_and_swap(
                    bucket_ptr,
                    tombstone as _,
                    Ordering::SeqCst,
                ) == bucket_ptr
                {
                    self.remaining_cells.fetch_add(1, Ordering::SeqCst);
                    defer(self.era, move || sarc_remove_copy(bucket_ptr));
                    return true;
                } else {
                    continue;
                }
            } else {
                idx = incr_idx(self, idx);
            }
        }
    }

    fn get_elem<'a, Q>(&'a self, key: &Q) -> Option<*mut ABox<Element<K, V>>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let buckets = self.buckets();
        let hash = do_hash(&*self.hash_builder, key);
        let mut idx = hash2idx(hash, self.capacity);

        loop {
            let bucket_ptr = unsafe { buckets.get_unchecked(idx).load(Ordering::Relaxed) };
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    if let Some(elem) = self.get_next().unwrap().get_elem(key) {
                        return Some(elem);
                    } else {
                        idx = incr_idx(self, idx);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    idx = incr_idx(self, idx);
                    continue;
                }

                _ => (),
            }

            if bucket_ptr.is_null() {
                return None;
            }

            let bucket_data = sarc_deref(bucket_ptr);
            if key == bucket_data.key.borrow() {
                return Some(bucket_ptr);
            } else {
                idx = incr_idx(self, idx);
            }
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.get_elem(key).map(Element::read)
    }
}

impl<K, V, S> Drop for Table<K, V, S> {
    fn drop(&mut self) {
        protected(|| {
            let root_ptr = self.root.load(Ordering::SeqCst);
            defer(self.era, move || {
                ba_drop(root_ptr);
            });
        });
    }
}

pub struct Table<K, V, S> {
    hash_builder: Arc<S>,
    era: usize,
    root: AtomicPtr<BucketArray<K, V, S>>,
}

impl<K: Eq + Hash, V, S: BuildHasher> Table<K, V, S> {
    pub fn new(capacity: usize, hash_builder: Arc<S>, era: usize) -> Self {
        let root = AtomicPtr::new(BucketArray::new(capacity, hash_builder.clone(), era));
        Self {
            hash_builder,
            root,
            era,
        }
    }

    fn root<'a>(&self) -> &'a BucketArray<K, V, S> {
        unsafe { &*self.root.load(Ordering::SeqCst) }
    }

    pub fn insert(&self, key: K, hash: u64, value: V) -> bool {
        let node = sarc_new(Element::new(key, hash, value));
        protected(|| {
            let root = self.root();
            let (maybe_new_root, did_replace) = root.insert_node(node);
            if let Some(new_root) = maybe_new_root {
                self.root.store(new_root as _, Ordering::SeqCst);
                defer(self.era, move || {
                    ba_drop(root as *const BucketArray<K, V, S> as *mut BucketArray<K, V, S>);
                });
            }
            did_replace
        })
    }

    pub fn get<Q>(&self, key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.root().get(key))
    }

    pub fn extract<T, Q, F>(&self, key: &Q, do_extract: F) -> Option<T>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnOnce(&K, &V) -> T,
    {
        protected(|| self.root().extract(key, do_extract))
    }

    pub fn remove<'a, Q>(&'a self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.root().remove(key))
    }
}

impl<K: Eq + Hash + Clone, V, S: BuildHasher> Table<K, V, S> {
    pub fn optimistic_update<Q, F>(&self, key: &Q, do_update: &mut F) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        protected(|| self.root().optimistic_update(key, do_update))
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for Table<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for Table<K, V, S> {}
