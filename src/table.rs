use super::element::*;
use crate::alloc::{ABox, sarc_add_copy, sarc_deref, sarc_remove_copy, sarc_new};
use crate::util::CachePadded;
use crate::recl::{protected, defer, collect};
use crate::pointer::{p_tag, p_set_tag};
use std::borrow::Borrow;
use std::cmp;
use std::fmt::Debug;
use std::hash::{BuildHasher, Hash, Hasher};
use std::iter;
use std::mem::{self, ManuallyDrop};
use std::sync::atomic::{AtomicUsize, Ordering, AtomicPtr};
use std::sync::Arc;

const REDIRECT_TAG: usize = 5;
const TOMBSTONE_TAG: usize = 7;

fn make_buckets<K, V>(x: usize) -> Box<[AtomicPtr<ABox<Element<K, V>>>]> {
    iter::repeat(0 as _).map(|p| AtomicPtr::new(p)).take(x).collect()
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
    hash2idx(i as u64 + 1, s.buckets.len())
}

enum InsertResult {
    None,
    Grow,
}

impl<K, V, S> Drop for BucketArray<K, V, S> {
    fn drop(&mut self) {
        protected(|| {
            let mut garbage = Vec::with_capacity(self.buckets.len());
            unsafe {
                for bucket in &*self.buckets {
                    let ptr = p_set_tag(bucket.load(Ordering::SeqCst), 0);
                    if !ptr.is_null() {
                        garbage.push(ptr);
                    }
                }
                defer(|| {
                    for ptr in garbage {
                        sarc_remove_copy(ptr);
                    }
                });
            }
        });
    }
}

pub struct BucketArray<K, V, S> {
    remaining_cells: CachePadded<AtomicUsize>,
    next: AtomicPtr<Self>,
    hash_builder: Arc<S>,
    buckets: Box<[AtomicPtr<ABox<Element<K, V>>>]>,
}

impl<K: Eq + Hash + Debug, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(mut capacity: usize, hash_builder: Arc<S>) -> Self {
        capacity = 2 * capacity; // TO-DO: remove this once grow works
        let remaining_cells = CachePadded::new(AtomicUsize::new(cmp::min(capacity * 3 / 4, capacity)));
        let buckets = make_buckets(capacity);

        Self {
            remaining_cells,
            hash_builder,
            buckets,
            next: AtomicPtr::new(0 as _),
        }
    }

    fn insert_node<'a>(
        &self,
        node: *mut ABox<Element<K, V>>,
    ) -> Option<*const Self> {
        if let Some(next) = self.get_next() {
            return next.insert_node(node);
        }

        let node_data = sarc_deref(node);
        let mut idx = hash2idx(node_data.hash, self.buckets.len());

        loop {
            let current_bucket_ptr = self.buckets[idx].load(Ordering::SeqCst);
            match p_tag(current_bucket_ptr) {
                REDIRECT_TAG => {
                    return self.get_next()
                        .unwrap()
                        .insert_node(node);
                }

                TOMBSTONE_TAG => {
                    if self.buckets[idx].compare_and_swap(current_bucket_ptr, node, Ordering::SeqCst) == current_bucket_ptr {
                        cell_maybe_return!(self);
                    } else {
                        continue;
                    }
                }

                _ => (),
            }

            if !current_bucket_ptr.is_null() {
                let current_bucket_data = sarc_deref(current_bucket_ptr);
                if current_bucket_data.key == node_data.key {
                    if self.buckets[idx].compare_and_swap(current_bucket_ptr, node, Ordering::SeqCst) == current_bucket_ptr {
                        defer(|| sarc_remove_copy(current_bucket_ptr));
                        cell_maybe_return!(self);
                    } else {
                        continue;
                    }
                } else {
                    idx = incr_idx(self, idx);
                    continue;
                }
            } else {
                if self.buckets[idx].compare_and_swap(current_bucket_ptr, node, Ordering::SeqCst) == current_bucket_ptr {
                    cell_maybe_return!(self);
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
                if self.buckets[idx].compare_and_swap(bucket_ptr, null as _, Ordering::SeqCst) == bucket_ptr {
                    self.remaining_cells.fetch_add(1, Ordering::SeqCst);
                    defer(|| sarc_remove_copy(bucket_ptr));
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

    fn get<Q>(&self, key: &Q) -> Option<ElementReadGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let r = self.get_elem(key).map(|ptr| Element::read(ptr));
        collect();
        r
    }
}

impl<K, V, S> Drop for Table<K, V, S> {
    fn drop(&mut self) {
        protected(|| {
            let root_ptr = self.root.load(Ordering::SeqCst);
            unsafe {
                defer(|| drop(Box::from_raw(root_ptr)));
            }
        });
    }
}

pub struct Table<K, V, S> {
    hash_builder: Arc<S>,
    root: AtomicPtr<BucketArray<K, V, S>>,
}

impl<K: Eq + Hash + Debug, V, S: BuildHasher> Table<K, V, S> {
    pub fn new(capacity: usize, hash_builder: Arc<S>) -> Self {
        let root = AtomicPtr::new(Box::into_raw(Box::new(BucketArray::new(capacity, hash_builder.clone()))));
        Self { hash_builder, root }
    }

    fn root<'a>(&self) -> &'a BucketArray<K, V, S> {
        unsafe { &*self.root.load(Ordering::SeqCst) }
    }

    pub fn insert(&self, key: K, hash: u64, value: V) {
        protected(|| {
            let node = sarc_new(Element::new(key, hash, value));
            let root = self.root();
            if let Some(new_root) = root.insert_node(node) {
                self.root.store(new_root as _, Ordering::SeqCst);
                defer(|| unsafe {
                    drop(Box::from_raw(root as *const BucketArray<K, V, S> as *mut BucketArray<K, V, S>));
                });
            }
        });
    }

    pub fn get<Q>(&self, key: &Q) -> Option<ElementReadGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.root().get(key))
    }

    pub fn remove<'a, Q>(&'a self, key: &Q)
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        protected(|| self.root().remove(key));
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for Table<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for Table<K, V, S> {}
