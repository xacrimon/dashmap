use super::element::*;
use crossbeam_epoch::{pin, Atomic, Guard, Owned, Shared};
use std::borrow::Borrow;
use std::cmp;
use std::fmt::Debug;
use std::hash::{BuildHasher, Hash, Hasher};
use std::iter;
use std::mem;
use std::sync::atomic::{fence, AtomicUsize, Ordering};
use std::sync::Arc;

const REDIRECT_TAG: usize = 5;
const TOMBSTONE_TAG: usize = 7;

pub fn make_shift(x: usize) -> usize {
    debug_assert!(x.is_power_of_two());
    x
}

fn make_buckets<K, V>(x: usize) -> Box<[Atomic<Element<K, V>>]> {
    iter::repeat(Atomic::null()).take(x).collect()
}

pub fn hash2idx(hash: u64, shift: usize) -> usize {
    //dbg!(hash as usize % shift);
    hash as usize % shift
}

pub fn do_hash(f: &impl BuildHasher, i: &(impl ?Sized + Hash)) -> u64 {
    let mut hasher = f.build_hasher();
    i.hash(&mut hasher);
    hasher.finish()
}

macro_rules! cell_maybe_return {
    ($s:expr, $g:expr) => {
        return if $s.remaining_cells.fetch_sub(1, Ordering::SeqCst) == 1 {
            $s.grow($g)
        } else {
            None
        };
    };
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
        let guard = &pin();
        let mut garbage = Vec::with_capacity(self.buckets.len());
        fence(Ordering::SeqCst);
        unsafe {
            for bucket in &*self.buckets {
                let ptr = bucket.load(Ordering::Relaxed, guard).into_owned();
                garbage.push(ptr);
            }
            guard.defer_unchecked(|| drop(garbage));
        }
    }
}

pub struct BucketArray<K, V, S> {
    remaining_cells: AtomicUsize,
    shift: usize,
    hash_builder: Arc<S>,
    buckets: Box<[Atomic<Element<K, V>>]>,
    next: Atomic<Self>,
}

impl<K: Eq + Hash + Debug, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(mut capacity: usize, hash_builder: Arc<S>) -> Self {
        capacity = 2 * capacity;
        //dbg!(capacity);
        let remaining_cells = AtomicUsize::new(cmp::min(capacity * 3 / 4, capacity));
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
        node: Shared<Element<K, V>>,
    ) -> Option<Shared<'a, Self>> {
        if let Some(next) = self.get_next(guard) {
            return next.insert_node(guard, node);
        }

        let inner = unsafe { node.deref() };
        //dbg!("enter");

        let mut idx = hash2idx(inner.hash, self.shift);
        let mut node = Some(node);

        loop {
            //dbg!("loop start");
            let e_current = self.buckets[idx].load(Ordering::Acquire, guard);
            match e_current.tag() {
                REDIRECT_TAG => {
                    //dbg!("was redirect, delegating");
                    self.get_next(guard)
                        .unwrap()
                        .insert_node(guard, node.take().unwrap());

                    return None;
                }

                TOMBSTONE_TAG => {
                    //dbg!("was tombstone");

                    match {
                        self.buckets[idx].compare_and_set(
                            e_current,
                            node.take().unwrap(),
                            Ordering::AcqRel,
                            guard,
                        )
                    } {
                        Ok(_) => cell_maybe_return!(self, guard),
                        Err(err) => {
                            node = Some(err.new);
                            continue;
                        }
                    }
                }

                _ => (),
            }
            if let Some(e_current_node) = unsafe { e_current.as_ref() } {
                //dbg!("encountered filled bucket");

                if e_current_node.hash == inner.hash && e_current_node.key == inner.key {
                    //dbg!("bucket key matched");
                    match {
                        self.buckets[idx].compare_and_set(
                            e_current,
                            node.take().unwrap(),
                            Ordering::AcqRel,
                            guard,
                        )
                    } {
                        Ok(_) => cell_maybe_return!(self, guard),
                        Err(err) => {
                            node = Some(err.new);
                            continue;
                        }
                    }
                } else {
                    idx = incr_idx(self, idx);
                    //dbg!("bucket key did not match", idx);
                    continue;
                }
            } else {
                //dbg!("was null, cas 1");
                match {
                    self.buckets[idx].compare_and_set(
                        e_current,
                        node.take().unwrap(),
                        Ordering::AcqRel,
                        guard,
                    )
                } {
                    Ok(_) => cell_maybe_return!(self, guard),
                    Err(err) => {
                        node = Some(err.new);
                        continue;
                    }
                }
            }
        }
    }

    fn grow<'a>(&self, guard: &'a Guard) -> Option<Shared<'a, Self>> {
        panic!("growth call");
        let shared = self.next.load(Ordering::SeqCst, guard);

        if unsafe { shared.as_ref().is_some() } {
            return None;
        }

        let new_cap = self.buckets.len() * 2;
        let new = Owned::new(Self::new(new_cap, self.hash_builder.clone())).into_shared(guard);
        let new_i = unsafe { new.deref() };

        if self
            .next
            .compare_and_set(shared, new, Ordering::SeqCst, guard)
            .is_err()
        {
            return None;
        }

        for atomic in &*self.buckets {
            loop {
                let maybe_node = atomic.load(Ordering::SeqCst, guard);
                if atomic
                    .compare_and_set(
                        maybe_node,
                        maybe_node.with_tag(REDIRECT_TAG),
                        Ordering::SeqCst,
                        guard,
                    )
                    .is_err()
                {
                    continue;
                }
                new_i.insert_node(guard, maybe_node);
                break;
            }
        }

        Some(new)
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
            let shared = self.buckets[idx].load(Ordering::SeqCst, guard);
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
            let elem = unsafe { shared.as_ref().unwrap() };
            if hash == elem.hash && key == elem.key.borrow() {
                if self.buckets[idx]
                    .compare_and_set(
                        shared,
                        Shared::null().with_tag(TOMBSTONE_TAG),
                        Ordering::SeqCst,
                        guard,
                    )
                    .is_ok()
                {
                    self.remaining_cells.fetch_add(1, Ordering::SeqCst);
                    unsafe {
                        guard.defer_destroy(shared);
                    }
                    return true;
                } else {
                    continue;
                }
            } else {
                idx = incr_idx(self, idx);
            }
        }
    }

    fn get_elem<'a, Q>(&'a self, guard: &'a Guard, key: &Q) -> Option<&'a Element<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        if let Some(Some(elem)) = self.get_next(guard).map(|a| a.get_elem(guard, key)) {
            return Some(elem);
        }

        let hash = do_hash(&*self.hash_builder, key);
        let mut idx = hash2idx(hash, self.shift);

        loop {
            let shared = self.buckets[idx].load(Ordering::SeqCst, guard);
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
            let elem = unsafe { shared.as_ref().unwrap() };
            if hash == elem.hash && key == elem.key.borrow() {
                return Some(elem);
            } else {
                idx = incr_idx(self, idx);
            }
        }
    }

    fn get<'a, Q>(&'a self, guard: &'a Guard, key: &Q) -> Option<ElementReadGuard<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.get_elem(guard, key).map(|e| e.read(pin()))
    }

    fn get_mut<'a, Q>(&'a self, guard: &'a Guard, key: &Q) -> Option<ElementWriteGuard<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.get_elem(guard, key).map(|e| e.write(pin()))
    }
}

impl<K, V, S> Drop for Table<K, V, S> {
    fn drop(&mut self) {
        let guard = pin();
        let shared = self.root.load(Ordering::Acquire, &guard);
        unsafe {
            guard.defer_destroy(shared);
        }
    }
}

pub struct Table<K, V, S> {
    hash_builder: Arc<S>,
    root: Atomic<BucketArray<K, V, S>>,
}

impl<K: Eq + Hash + Debug, V, S: BuildHasher> Table<K, V, S> {
    pub fn new(capacity: usize, hash_builder: Arc<S>) -> Self {
        let root = Atomic::new(BucketArray::new(capacity, hash_builder.clone()));
        Self { hash_builder, root }
    }

    fn root<'a>(&self, guard: &'a Guard) -> &'a BucketArray<K, V, S> {
        unsafe { self.root.load(Ordering::Acquire, guard).deref() }
    }

    pub fn insert(&self, key: K, hash: u64, value: V) {
        let guard = pin();
        let node = Owned::new(Element::new(key, hash, value)).into_shared(&guard);
        let root = self.root(&guard);
        if let Some(new_root) = root.insert_node(&guard, node) {
            self.root.store(new_root, Ordering::SeqCst);
            unsafe {
                let prev_shared: Shared<'_, BucketArray<K, V, S>> = mem::transmute(root);
                guard.defer_destroy(prev_shared);
            }
        }
    }

    pub fn get<'a, Q>(&'a self, key: &Q) -> Option<ElementReadGuard<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let guard = pin();
        unsafe { mem::transmute(self.root(&guard).get(&guard, key)) }
    }

    pub fn get_mut<'a, Q>(&'a self, key: &Q) -> Option<ElementWriteGuard<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let guard = pin();
        unsafe { mem::transmute(self.root(&guard).get_mut(&guard, key)) }
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
