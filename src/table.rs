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
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

const REDIRECT_TAG: usize = 5;
const TOMBSTONE_TAG: usize = 7;

pub fn hash2idx(hash: u64, len: usize) -> (usize, usize) {
    let group_len = len / 8;
    let group_idx = (hash as usize / 8) % (group_len + 1);
    let private_idx = hash as usize % 8;
    (group_idx, private_idx)
}

pub fn do_hash(f: &impl BuildHasher, i: &(impl ?Sized + Hash)) -> u64 {
    let mut hasher = f.build_hasher();
    i.hash(&mut hasher);
    hasher.finish()
}

fn lower8(x: u64) -> u8 {
    (x & 0xFF) as u8
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

fn incr_idx<K, V, S>(s: &BucketArray<K, V, S>, gi: usize, pi: usize) -> (usize, usize) {
    match pi {
        7 => ((gi + 1) & (s.groups().len() - 1), 0),
        pi => (gi, pi + 1),
    }
}

enum InsertResult {
    None,
    Grow,
}

impl<K, V, S> Drop for BucketArray<K, V, S> {
    fn drop(&mut self) {
        let mut garbage = Vec::with_capacity(self.capacity);
        let groups = self.groups();
        for group in &*groups {
            for bucket in &group.buckets {
                let ptr = p_set_tag(bucket.load(Ordering::SeqCst), 0);
                if !ptr.is_null() {
                    garbage.push(ptr);
                }
            }
        }
        defer(self.era, move || {
            for ptr in garbage {
                sarc_remove_copy(ptr);
            }
        });
    }
}

struct Group<K, V> {
    cached_hashes: [AtomicU8; 8],
    buckets: [AtomicPtr<ABox<Element<K, V>>>; 8],
}

impl<K, V> Group<K, V> {
    fn new() -> Self {
        Self {
            cached_hashes: [
                AtomicU8::new(0),
                AtomicU8::new(0),
                AtomicU8::new(0),
                AtomicU8::new(0),
                AtomicU8::new(0),
                AtomicU8::new(0),
                AtomicU8::new(0),
                AtomicU8::new(0),
            ],
            buckets: [
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

    fn fetch(&self, pi: usize) -> (&AtomicPtr<ABox<Element<K, V>>>, &AtomicU8) {
        (&self.buckets[pi], &self.cached_hashes[pi])
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
        let groups = self.groups();
        let hash = do_hash(&*self.hash_builder, search_key);
        let mut gipi = hash2idx(hash, self.capacity);

        loop {
            let (atomic_bucket, _) = groups[gipi.0].fetch(gipi.1);
            let bucket_ptr = atomic_bucket.load(Ordering::SeqCst);
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    if self
                        .get_next()
                        .unwrap()
                        .optimistic_update(search_key, do_update)
                    {
                        return true;
                    } else {
                        gipi = incr_idx(self, gipi.0, gipi.1);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    gipi = incr_idx(self, gipi.0, gipi.1);
                    continue;
                }

                _ => (),
            }
            if bucket_ptr.is_null() {
                return false;
            }
            let bucket_data = sarc_deref(bucket_ptr);
            if search_key == bucket_data.key.borrow() {
                let v = do_update(&bucket_data.key, &bucket_data.value);
                let k = bucket_data.key.clone();
                let new_node = sarc_new(Element::new(k, bucket_data.hash, v));
                if atomic_bucket.compare_and_swap(bucket_ptr, new_node, Ordering::SeqCst)
                    == bucket_ptr
                {
                    defer(self.era, move || sarc_remove_copy(bucket_ptr));
                    return true;
                } else {
                    sarc_remove_copy(new_node);
                    continue;
                }
            } else {
                gipi = incr_idx(self, gipi.0, gipi.1);
                continue;
            }
        }
    }
}

fn ba_layout<K, V, S>(group_amount: usize) -> Layout {
    let start_size = mem::size_of::<BucketArray<K, V, S>>();
    let align = mem::align_of::<BucketArray<K, V, S>>();
    let total_size = start_size + group_amount * mem::size_of::<Group<K, V>>();
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
    fn groups<'a>(&'a self) -> &'a [Group<K, V>] {
        let self_begin = self as *const _ as usize as *mut u8;
        unsafe {
            let array_begin = self_begin.add(mem::size_of::<Self>());
            slice::from_raw_parts(array_begin as _, self.capacity)
        }
    }
}

static LOAD_FACTOR: f32 = 0.75;

fn calc_cells_remaining(capacity: usize) -> usize {
    (LOAD_FACTOR * capacity as f32) as usize
}

fn calc_capacity(capacity: usize) -> usize {
    let fac = 2.0 - LOAD_FACTOR;
    (capacity as f32 * fac) as usize
}

fn round_8mul(x: usize) -> usize {
    (x + 7) & !7
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(mut capacity: usize, hash_builder: Arc<S>, era: usize) -> *mut Self {
        unsafe {
            capacity = round_8mul(calc_capacity(cmp::max(capacity, 8)));
            let remaining_cells =
                CachePadded::new(AtomicUsize::new(calc_cells_remaining(capacity)));
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
            for i in 0..(capacity / 8) {
                let p2p = (array_start as *mut Group<K, V>).add(i);
                *p2p = Group::new();
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
        if let Some(next) = self.get_next() {
            return next.insert_node(node);
        }

        let groups = self.groups();
        let node_data = sarc_deref(node);
        let hash_cache = lower8(node_data.hash);
        let mut gipi = hash2idx(node_data.hash, self.capacity);

        loop {
            let (atomic_bucket, atomic_cache) = groups[gipi.0].fetch(gipi.1);
            let cached = atomic_cache.load(Ordering::SeqCst);
            let bucket_ptr = atomic_bucket.load(Ordering::SeqCst);
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    return self.get_next().unwrap().insert_node(node);
                }

                TOMBSTONE_TAG => {
                    if hash_cache != cached {
                        gipi = incr_idx(self, gipi.0, gipi.1);
                        continue;
                    }

                    if atomic_cache.compare_and_swap(cached, hash_cache, Ordering::SeqCst) != cached
                    {
                        continue;
                    }
                    if atomic_bucket.compare_and_swap(bucket_ptr, node, Ordering::SeqCst)
                        == bucket_ptr
                    {
                        cell_maybe_return!(self, false);
                    } else {
                        continue;
                    }
                }

                _ => (),
            }
            if !bucket_ptr.is_null() {
                if hash_cache != cached {
                    gipi = incr_idx(self, gipi.0, gipi.1);
                    continue;
                }
                let bucket_data = sarc_deref(bucket_ptr);
                if bucket_data.key == node_data.key {
                    if atomic_cache.compare_and_swap(cached, hash_cache, Ordering::SeqCst) != cached
                    {
                        continue;
                    }
                    if atomic_bucket.compare_and_swap(bucket_ptr, node, Ordering::SeqCst)
                        == bucket_ptr
                    {
                        defer(self.era, move || sarc_remove_copy(bucket_ptr));
                        cell_maybe_return!(self, true);
                    } else {
                        continue;
                    }
                } else {
                    gipi = incr_idx(self, gipi.0, gipi.1);
                    continue;
                }
            } else {
                if atomic_cache.compare_and_swap(cached, hash_cache, Ordering::SeqCst) != cached {
                    continue;
                }
                if atomic_bucket.compare_and_swap(bucket_ptr, node, Ordering::SeqCst) == bucket_ptr
                {
                    cell_maybe_return!(self, false);
                } else {
                    continue;
                }
            }
        }
    }

    fn insert_g_grow<'a>(&self, node: *mut ABox<Element<K, V>>) -> Option<*const Self> {
        if let Some(next) = self.get_next() {
            return next.insert_g_grow(node);
        }

        let groups = self.groups();
        let node_data = sarc_deref(node);
        let hash_cache = lower8(node_data.hash);
        let mut gipi = hash2idx(node_data.hash, self.capacity);

        loop {
            let (atomic_bucket, atomic_cache) = groups[gipi.0].fetch(gipi.1);
            let cached = atomic_cache.load(Ordering::SeqCst);
            let bucket_ptr = atomic_bucket.load(Ordering::SeqCst);
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    return self.get_next().unwrap().insert_g_grow(node);
                }

                TOMBSTONE_TAG => {
                    if hash_cache != cached {
                        gipi = incr_idx(self, gipi.0, gipi.1);
                        continue;
                    }

                    if atomic_cache.compare_and_swap(cached, hash_cache, Ordering::SeqCst) != cached
                    {
                        continue;
                    }
                    if atomic_bucket.compare_and_swap(bucket_ptr, node, Ordering::SeqCst)
                        == bucket_ptr
                    {
                        cell_maybe_return_k3!(self);
                    } else {
                        continue;
                    }
                }

                _ => (),
            }
            if !bucket_ptr.is_null() {
                if hash_cache != cached {
                    gipi = incr_idx(self, gipi.0, gipi.1);
                    continue;
                }
                let bucket_data = sarc_deref(bucket_ptr);
                if bucket_data.key == node_data.key {
                    return None;
                } else {
                    gipi = incr_idx(self, gipi.0, gipi.1);
                    continue;
                }
            } else {
                if atomic_cache.compare_and_swap(cached, hash_cache, Ordering::SeqCst) != cached {
                    continue;
                }
                if atomic_bucket.compare_and_swap(bucket_ptr, node, Ordering::SeqCst) == bucket_ptr
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
        for group in self.groups() {
            for atomic_bucket in &group.buckets {
                loop {
                    let bucket_ptr = atomic_bucket.load(Ordering::SeqCst);
                    let redirect_variant = p_set_tag(bucket_ptr, REDIRECT_TAG);
                    if atomic_bucket.compare_and_swap(
                        bucket_ptr,
                        redirect_variant,
                        Ordering::SeqCst,
                    ) != bucket_ptr
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
        let groups = self.groups();
        let hash = do_hash(&*self.hash_builder, key);
        let mut gipi = hash2idx(hash, self.capacity);

        loop {
            let (atomic_bucket, _) = groups[gipi.0].fetch(gipi.1);
            let bucket_ptr = atomic_bucket.load(Ordering::SeqCst);
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    if self.get_next().unwrap().remove(key) {
                        return true;
                    } else {
                        gipi = incr_idx(self, gipi.0, gipi.1);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    gipi = incr_idx(self, gipi.0, gipi.1);
                    continue;
                }

                _ => (),
            }
            if bucket_ptr.is_null() {
                return false;
            } else if key == sarc_deref(bucket_ptr).key.borrow() {
                let tombstone = p_set_tag(ptr::null_mut(), TOMBSTONE_TAG);
                if atomic_bucket.compare_and_swap(bucket_ptr, tombstone, Ordering::SeqCst)
                    == bucket_ptr
                {
                    defer(self.era, move || sarc_remove_copy(bucket_ptr));
                    return true;
                } else {
                    continue;
                }
            } else {
                gipi = incr_idx(self, gipi.0, gipi.1);
                continue;
            }
        }
    }

    fn get_elem<'a, Q>(&'a self, key: &Q) -> Option<*mut ABox<Element<K, V>>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let groups = self.groups();
        let hash = do_hash(&*self.hash_builder, key);
        let hash_cache = lower8(hash);
        let mut gipi = hash2idx(hash, self.capacity);

        loop {
            let (atomic_bucket, atomic_cache) = groups[gipi.0].fetch(gipi.1);
            let cached = atomic_cache.load(Ordering::SeqCst);
            let bucket_ptr = atomic_bucket.load(Ordering::SeqCst);
            match p_tag(bucket_ptr) {
                REDIRECT_TAG => {
                    if let Some(elem) = self.get_next().unwrap().get_elem(key) {
                        return Some(elem);
                    } else {
                        gipi = incr_idx(self, gipi.0, gipi.1);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    gipi = incr_idx(self, gipi.0, gipi.1);
                    continue;
                }

                _ => (),
            }
            if bucket_ptr.is_null() {
                return None;
            }
            if cached != atomic_cache.load(Ordering::SeqCst) {
                continue;
            }
            if cached != hash_cache {
                gipi = incr_idx(self, gipi.0, gipi.1);
                continue;
            }
            let bucket_data = sarc_deref(bucket_ptr);
            if key == bucket_data.key.borrow() {
                return Some(bucket_ptr as _);
            } else {
                gipi = incr_idx(self, gipi.0, gipi.1);
                continue;
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
