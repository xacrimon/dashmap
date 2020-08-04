use super::{EntryManager, NewEntryState};
use crate::alloc::ObjectAllocator;
use crate::bucket::Bucket;
use std::borrow::Borrow;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

fn strip(x: usize) -> usize {
    x & !0b11
}

fn is_on(field: usize, idx: usize) -> bool {
    field & (1 << idx) != 0
}

pub struct GenericEntryManager<K, V> {
    _marker_a: PhantomData<K>,
    _marker_b: PhantomData<V>,
}

impl<K: 'static + Eq + Hash, V: 'static> EntryManager for GenericEntryManager<K, V> {
    type K = K;
    type V = V;

    fn empty() -> AtomicUsize {
        AtomicUsize::new(0)
    }

    fn is_null(entry: usize) -> bool {
        entry == 0
    }

    fn is_tombstone(entry: usize) -> bool {
        is_on(entry, 0)
    }

    fn is_resize(entry: usize) -> bool {
        is_on(entry, 1)
    }

    fn eq<Q, A>(entry: usize, other: &Q) -> bool
    where
        Self::K: Borrow<Q>,
        Q: ?Sized + Eq,
        A: ObjectAllocator<Bucket<Self::K, Self::V, A>>,
    {
        if Self::is_null(entry) {
            false
        } else {
            let bucket_ptr = strip(entry) as *const Bucket<K, V, A>;
            let bucket = unsafe { &*bucket_ptr };
            bucket.key.borrow() == other
        }
    }

    fn cas<F, A>(entry: &AtomicUsize, f: F) -> bool
    where
        F: FnOnce(
            usize,
            Option<(*const Self::K, *const Self::V)>,
        ) -> NewEntryState<Self::K, Self::V, A>,
        A: ObjectAllocator<Bucket<Self::K, Self::V, A>>,
    {
        todo!()
    }
}
