use super::element::*;
use crate::alloc::{ABox, sarc_add_copy, sarc_deref, sarc_remove_copy};
use crate::util::CachePadded;
use crate::recl::{enter_critical, exit_critical, defer, collect};
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

pub fn make_shift(x: usize) -> usize {
    debug_assert!(x.is_power_of_two());
    x
}

fn make_buckets<K, V>(x: usize) -> Box<[AtomicPtr<ABox<Element<K, V>>>]> {
    iter::repeat(AtomicPtr::new(0 as _)).take(x).collect()
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
    ($s:expr, $g:expr) => {{
        let should_grow = $s.remaining_cells.fetch_sub(1, Ordering::SeqCst) == 1;
        if should_grow {
            return $s.grow($g);
        } else {
            return None;
        }
    }};
}

fn incr_idx<K, V, S>(s: &BucketArray<K, V, S>, i: usize) -> usize {
    hash2idx(i as u64 + 1, s.shift)
}

enum InsertResult {
    None,
    Grow,
}

impl<K, V, S> Drop for BucketArray<K, V, S> {
    fn drop(&mut self) {
        enter_critical();
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
        exit_critical();
    }
}

pub struct BucketArray<K, V, S> {
    remaining_cells: CachePadded<AtomicUsize>,
    next: AtomicPtr<Self>,
    shift: usize,
    hash_builder: Arc<S>,
    buckets: Box<[AtomicPtr<ABox<Element<K, V>>>]>,
}

impl<K: Eq + Hash + Debug, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(mut capacity: usize, hash_builder: Arc<S>) -> Self {
        capacity = 2 * capacity; // TO-DO: remove this once grow works
        let remaining_cells = CachePadded::new(AtomicUsize::new(cmp::min(capacity * 3 / 4, capacity)));
        let shift = make_shift(capacity);
        let buckets = make_buckets(capacity);

        Self {
            remaining_cells,
            shift,
            hash_builder,
            buckets,
            next: Atomic::null(),
        }
    }

    fn insert_node<'a>(
        &self,
        guard: &'a Guard,
        node: *const ABox<Element<K, V>>,
    ) {
        if let Some(next) = self.get_next(guard) {
            return next.insert_node(guard, node);
        }

        let mut idx = hash2idx(node.hash, self.shift);
        let inner = node.clone();
        let mut node = Some(node);

        loop {
            let e_current = self.buckets[idx].load(Ordering::SeqCst, guard);
            match e_current.tag() {
                REDIRECT_TAG => {
                    self.get_next(guard)
                        .unwrap()
                        .insert_node(guard, node.take().unwrap());

                    return None;
                }

                TOMBSTONE_TAG => {
                    match {
                        self.buckets[idx].compare_and_set(
                            e_current,
                            ManuallyDrop::into_inner(node.take().unwrap()).into_shared(guard),
                            Ordering::SeqCst,
                            guard,
                        )
                    } {
                        Ok(_) => cell_maybe_return!(self, guard),
                        Err(err) => {
                            node = unsafe { Some(ManuallyDrop::new(Sarc::from_shared(err.new))) };
                            continue;
                        }
                    }
                }

                _ => (),
            }
            if let Some(e_current_node) = unsafe { Sarc::from_shared_maybe(e_current) } {
                if e_current_node.key == inner.key {
                    match {
                        self.buckets[idx].compare_and_set(
                            e_current,
                            ManuallyDrop::into_inner(node.take().unwrap()).into_shared(guard),
                            Ordering::SeqCst,
                            guard,
                        )
                    } {
                        Ok(_) => {
                            unsafe {
                                guard.defer_unchecked(move || drop(Sarc::from_shared(e_current)))
                            }

                            cell_maybe_return!(self, guard)
                        }
                        Err(err) => {
                            node = unsafe { Some(ManuallyDrop::new(Sarc::from_shared(err.new))) };
                            continue;
                        }
                    }
                } else {
                    idx = incr_idx(self, idx);
                    continue;
                }
            } else {
                let s_new = ManuallyDrop::into_inner(node.take().unwrap()).into_shared(guard);
                match {
                    self.buckets[idx].compare_and_set(e_current, s_new, Ordering::AcqRel, guard)
                } {
                    Ok(_) => {
                        cell_maybe_return!(self, guard);
                    }
                    Err(err) => {
                        node = unsafe { Some(ManuallyDrop::new(Sarc::from_shared(err.new))) };
                        continue;
                    }
                }
            }
        }
    }

    fn grow<'a>(&self, guard: &'a Guard) -> Option<Shared<'a, Self>> {
        unimplemented!()
    }

    fn get_next<'a>(&self, guard: &'a Guard) -> Option<&'a Self> {
        unsafe { self.next.load(Ordering::Acquire, guard).as_ref() }
    }

    fn remove<Q>(&self, guard: &Guard, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let hash = do_hash(&*self.hash_builder, key);
        let mut idx = hash2idx(hash, self.shift);

        loop {
            let shared = self.buckets[idx].load(Ordering::Acquire, guard);
            match shared.tag() {
                REDIRECT_TAG => {
                    if self.get_next(guard).unwrap().remove(guard, key) {
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
            if shared.is_null() {
                return false;
            }
            let elem = unsafe { Sarc::from_shared(shared) };
            if key == elem.key.borrow() {
                if self.buckets[idx]
                    .compare_and_set(
                        shared,
                        Shared::null().with_tag(TOMBSTONE_TAG),
                        Ordering::AcqRel,
                        guard,
                    )
                    .is_ok()
                {
                    self.remaining_cells.fetch_add(1, Ordering::Relaxed);
                    unsafe { guard.defer_unchecked(move || drop(elem)) }
                    return true;
                } else {
                    mem::forget(elem);
                    continue;
                }
            } else {
                idx = incr_idx(self, idx);
            }
        }
    }

    fn get_elem<'a, Q>(&'a self, guard: &'a Guard, key: &Q) -> Option<Sarc<Element<K, V>>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let hash = do_hash(&*self.hash_builder, key);
        let mut idx = hash2idx(hash, self.shift);

        loop {
            let shared = self.buckets[idx].load(Ordering::Relaxed, guard);
            match shared.tag() {
                REDIRECT_TAG => {
                    if let Some(elem) = self.get_next(guard).unwrap().get_elem(guard, key) {
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
            if shared.is_null() {
                return None;
            }
            let elem = unsafe { Sarc::from_shared(shared) };
            if key == elem.key.borrow() {
                elem.incr();
                return Some(elem);
            } else {
                mem::forget(elem);
                idx = incr_idx(self, idx);
            }
        }
    }

    fn get<Q>(&self, guard: Guard, key: &Q) -> Option<ElementReadGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.get_elem(&guard, key).map(|e| Element::read(e))
    }

    fn get_mut<Q>(&self, guard: Guard, key: &Q) -> Option<ElementWriteGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.get_elem(&guard, key).map(|e| Element::write(e))
    }
}

impl<K, V, S> Drop for Table<K, V, S> {
    fn drop(&mut self) {
        let guard = pin();
        let shared = self.root.load(Ordering::SeqCst, &guard);
        unsafe {
            guard.defer_unchecked(move || Sanic::from_shared(shared));
        }
    }
}

pub struct Table<K, V, S> {
    hash_builder: Arc<S>,
    root: Atomic<BucketArray<K, V, S>>,
}

impl<K: Eq + Hash + Debug, V, S: BuildHasher> Table<K, V, S> {
    pub fn new(capacity: usize, hash_builder: Arc<S>) -> Self {
        let root = Sanic::atomic(BucketArray::new(capacity, hash_builder.clone()));
        Self { hash_builder, root }
    }

    fn root<'a>(&self, guard: &'a Guard) -> &'a BucketArray<K, V, S> {
        unsafe { self.root.load(Ordering::Acquire, guard).deref() }
    }

    pub fn insert(&self, key: K, hash: u64, value: V) {
        let guard = pin();
        let node = Sarc::new(Element::new(key, hash, value));
        let root = self.root(&guard);
        if let Some(new_root) = root.insert_node(&guard, ManuallyDrop::new(node)) {
            self.root.store(new_root, Ordering::SeqCst);
            unsafe {
                let prev_shared: Shared<'_, BucketArray<K, V, S>> = mem::transmute(root);
                guard.defer_unchecked(move || Sanic::from_shared(prev_shared));
            }
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<ElementReadGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let guard = pin();
        unsafe {
            let root: &'_ BucketArray<K, V, S> = mem::transmute(self.root(&guard));
            mem::transmute(root.get(guard, key))
        }
    }

    pub fn get_mut<Q>(&self, key: &Q) -> Option<ElementWriteGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let guard = pin();
        unsafe {
            let root: &'_ BucketArray<K, V, S> = mem::transmute(self.root(&guard));
            mem::transmute(root.get_mut(guard, key))
        }
    }

    pub fn remove<'a, Q>(&'a self, key: &Q)
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let guard = pin();
        self.root(&guard).remove(&guard, key);
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for Table<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for Table<K, V, S> {}
