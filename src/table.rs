use super::element::*;
use crate::alloc::{sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::recl::{defer, protected};
use crate::util::{p_set_tag, p_tag, CachePadded};
use std::borrow::Borrow;
use std::cmp;
use std::hash::{BuildHasher, Hash, Hasher};
use std::iter;

use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::Arc;

const REDIRECT_TAG: usize = 5;
const TOMBSTONE_TAG: usize = 7;

fn make_buckets<K, V>(x: usize) -> Box<[AtomicPtr<ABox<Element<K, V>>>]> {
    iter::repeat(0 as _)
        .map(|p| AtomicPtr::new(p))
        .take(x)
        .collect()
}

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

fn incr_idx<K, V, S>(s: &BucketArray<K, V, S>, i: usize) -> usize {
    hash2idx(i as u64 + 1, s.buckets.len())
}

enum InsertResult {
    None,
    Grow,
}

impl<K, V, S> Drop for BucketArray<K, V, S> {
    fn drop(&mut self) {
        protected(|| {
            let cap = self.buckets.len();
            let mut garbage = Vec::with_capacity(cap);
            for bucket in &*self.buckets {
                let ptr = p_set_tag(bucket.load(Ordering::SeqCst), 0);
                if !ptr.is_null() {
                    garbage.push(ptr);
                }
            }
            defer(move || {
                for ptr in garbage {
                    sarc_remove_copy(ptr);
                }
            });
        });
    }
}

pub struct BucketArray<K, V, S> {
    remaining_cells: CachePadded<AtomicUsize>,
    next: AtomicPtr<Self>,
    hash_builder: Arc<S>,
    buckets: Box<[AtomicPtr<ABox<Element<K, V>>>]>,
}

impl<K: Eq + Hash + Clone, V: Clone, S: BuildHasher> BucketArray<K, V, S> {
    fn optimistic_update<Q, F>(&self, search_key: &Q, do_update: &mut F) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, V) -> V,
    {
        let hash = do_hash(&*self.hash_builder, search_key);
        let mut idx = hash2idx(hash, self.buckets.len());

        loop {
            let bucket_ptr = self.buckets[idx].load(Ordering::SeqCst);
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
                let (key, hash, value) = bucket_data.clone().destructure();
                let new_value = do_update(&key, value);
                let new_element = Element::new(key, hash, new_value);
                let new_ptr = sarc_new(new_element);

                if self.buckets[idx].compare_and_swap(bucket_ptr, new_ptr, Ordering::SeqCst)
                    == bucket_ptr
                {
                    defer(move || sarc_remove_copy(bucket_ptr));
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

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(mut capacity: usize, hash_builder: Arc<S>) -> Self {
        capacity = cmp::max(2 * capacity, 16); // TO-DO: remove this once grow works
        let remaining_cells = CachePadded::new(AtomicUsize::new(capacity * 3 / 4));
        let buckets = make_buckets(capacity);

        Self {
            remaining_cells,
            hash_builder,
            buckets,
            next: AtomicPtr::new(0 as _),
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

        let node_data = sarc_deref(node);
        let mut idx = hash2idx(node_data.hash, self.buckets.len());

        loop {
            let current_bucket_ptr = self.buckets[idx].load(Ordering::SeqCst);
            match p_tag(current_bucket_ptr) {
                REDIRECT_TAG => {
                    return self.get_next().unwrap().insert_node(node);
                }

                TOMBSTONE_TAG => {
                    if self.buckets[idx].compare_and_swap(
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
                    if self.buckets[idx].compare_and_swap(
                        current_bucket_ptr,
                        node,
                        Ordering::SeqCst,
                    ) == current_bucket_ptr
                    {
                        defer(move || sarc_remove_copy(current_bucket_ptr));
                        cell_maybe_return!(self, true);
                    } else {
                        continue;
                    }
                } else {
                    idx = incr_idx(self, idx);
                    continue;
                }
            } else {
                if self.buckets[idx].compare_and_swap(current_bucket_ptr, node, Ordering::SeqCst)
                    == current_bucket_ptr
                {
                    cell_maybe_return!(self, false);
                } else {
                    continue;
                }
            }
        }
    }

    fn grow<'a>(&self) -> Option<*const Self> {
        unimplemented!()
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
        let hash = do_hash(&*self.hash_builder, key);
        let mut idx = hash2idx(hash, self.buckets.len());

        loop {
            let bucket_ptr = self.buckets[idx].load(Ordering::SeqCst);
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
                let null = p_set_tag::<()>(0 as _, TOMBSTONE_TAG);
                if self.buckets[idx].compare_and_swap(bucket_ptr, null as _, Ordering::SeqCst)
                    == bucket_ptr
                {
                    self.remaining_cells.fetch_add(1, Ordering::SeqCst);
                    defer(move || sarc_remove_copy(bucket_ptr));
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
        let hash = do_hash(&*self.hash_builder, key);
        let mut idx = hash2idx(hash, self.buckets.len());

        loop {
            let bucket_ptr = self.buckets[idx].load(Ordering::SeqCst);
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
        self.get_elem(key).map(|ptr| Element::read(ptr))
    }
}

impl<K, V, S> Drop for Table<K, V, S> {
    fn drop(&mut self) {
        protected(|| {
            let root_ptr = self.root.load(Ordering::SeqCst);
            unsafe {
                defer(move || {
                    drop(Box::from_raw(root_ptr));
                });
            }
        });
    }
}

pub struct Table<K, V, S> {
    hash_builder: Arc<S>,
    root: AtomicPtr<BucketArray<K, V, S>>,
}

impl<K: Eq + Hash, V, S: BuildHasher> Table<K, V, S> {
    pub fn new(capacity: usize, hash_builder: Arc<S>) -> Self {
        let root = AtomicPtr::new(Box::into_raw(Box::new(BucketArray::new(
            capacity,
            hash_builder.clone(),
        ))));
        Self { hash_builder, root }
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
                defer(move || unsafe {
                    drop(Box::from_raw(
                        root as *const BucketArray<K, V, S> as *mut BucketArray<K, V, S>,
                    ));
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

impl<K: Eq + Hash + Clone, V: Clone, S: BuildHasher> Table<K, V, S> {
    pub fn optimistic_update<Q, F>(&self, key: &Q, do_update: &mut F) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, V) -> V,
    {
        protected(|| self.root().optimistic_update(key, do_update))
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for Table<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for Table<K, V, S> {}
