use crate::alloc::ObjectAllocator;
use crate::bucket::Bucket;
use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;

pub enum NewEntryState<K: 'static + Eq + Hash, V: 'static, A: ObjectAllocator<Bucket<K, V, A>>> {
    Empty,
    Keep,
    SetResize,
    New(*mut Bucket<K, V, A>),
}

pub trait EntryManager {
    type K: 'static + Eq + Hash;
    type V: 'static;

    fn eq<Q>(entry: usize, other: &Q, other_hash: u64) -> bool
    where
        Self::K: Borrow<Q>,
        Q: ?Sized + Eq;

    fn cas<F, A: ObjectAllocator<Bucket<Self::K, Self::V, A>>>(entry: &AtomicUsize, f: F) -> bool
    where
        F: FnOnce(
            usize,
            Option<(*const Self::K, *const Self::V)>,
        ) -> NewEntryState<Self::K, Self::V, A>;
}
