use super::element::*;
use crossbeam_epoch::{pin, Atomic, Guard, Owned, Shared};
use std::borrow::Borrow;
use std::cmp;
use std::hash::{BuildHasher, Hash, Hasher};
use std::iter;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const PTR_SIZE_BITS: usize = mem::size_of::<usize>() * 8;
const REDIRECT_TAG: usize = 5;
const TOMBSTONE_TAG: usize = 7;

fn make_shift(x: usize) -> usize {
    debug_assert!(x.is_power_of_two());
    PTR_SIZE_BITS - x.trailing_zeros() as usize
}

fn make_buckets<K, V>(x: usize) -> Box<[Atomic<Element<K, V>>]> {
    iter::repeat(Atomic::null()).take(x).collect()
}

fn hash2idx(hash: u64, shift: usize) -> usize {
    hash as usize >> shift
}

fn do_hash(f: &impl BuildHasher, i: &(impl ?Sized + Hash)) -> u64 {
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

macro_rules! incr_idx {
    ($s:expr, $i:ident) => {
        $i = hash2idx($i as u64 + 1, $s.shift);
    }
}

enum InsertResult {
    None,
    Grow,
}

pub struct BucketArray<K, V, S> {
    remaining_cells: AtomicUsize,
    shift: usize,
    hash_builder: Arc<S>,
    buckets: Box<[Atomic<Element<K, V>>]>,
    next: Atomic<Self>,
}

impl<K: Eq + Hash, V, S: BuildHasher> BucketArray<K, V, S> {
    fn new(capacity: usize, hash_builder: Arc<S>) -> Self {
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

        let mut idx = hash2idx(inner.hash, self.shift);
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
                            node.take().unwrap(),
                            Ordering::SeqCst,
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
                if e_current_node.hash == inner.hash && e_current_node.key == inner.key {
                    match {
                        self.buckets[idx].compare_and_set(
                            e_current,
                            node.take().unwrap(),
                            Ordering::SeqCst,
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
                    incr_idx!(self, idx);
                    continue;
                }
            } else {
                match {
                    self.buckets[idx].compare_and_set(
                        e_current,
                        node.take().unwrap(),
                        Ordering::SeqCst,
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
        let shared = self.next.load(Ordering::SeqCst, guard);

        if unsafe { shared.as_ref().is_some() } {
            return None;
        }

        let new_cap = self.buckets.len() * 2;
        let new = Owned::new(Self::new(new_cap, self.hash_builder.clone())).into_shared(guard);
        let new_i = unsafe { new.deref() };

        if let Err(_) = self
            .next
            .compare_and_set(shared, new, Ordering::SeqCst, guard)
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
        unsafe { self.next.load(Ordering::SeqCst, guard).as_ref() }
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
                        incr_idx!(self, idx);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    incr_idx!(self, idx);
                    continue;
                }

                _ => (),
            }
            if shared.is_null() {
                return false;
            }
            let elem = unsafe { shared.as_ref().unwrap() };
            if hash == elem.hash && key == elem.key.borrow() {
                if let Ok(_) = self.buckets[idx].compare_and_set(
                    shared,
                    Shared::null().with_tag(TOMBSTONE_TAG),
                    Ordering::SeqCst,
                    guard,
                ) {
                    self.remaining_cells.fetch_add(1, Ordering::SeqCst);
                    return true;
                } else {
                    continue;
                }
            } else {
                incr_idx!(self, idx);
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
                        incr_idx!(self, idx);
                        continue;
                    }
                }

                TOMBSTONE_TAG => {
                    incr_idx!(self, idx);
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
                incr_idx!(self, idx);
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

pub struct Table<K, V, S> {
    root: Atomic<BucketArray<K, V, S>>,
}
