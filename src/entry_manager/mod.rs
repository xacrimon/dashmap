mod generic;

use crate::alloc::ObjectAllocator;
use crate::bucket::Bucket;
use crate::gc::Gc;
use crate::utils::shim::sync::atomic::AtomicUsize;
use std::borrow::Borrow;
use std::hash::Hash;

/// The new state of a bucket after a compare-and-swap operation.
pub enum NewEntryState<K: 'static + Eq + Hash, V: 'static, A: ObjectAllocator<Bucket<K, V, A>>> {
    Empty,
    Keep,
    SetResize,
    New(*mut Bucket<K, V, A>),
}

pub trait EntryManager {
    type K: 'static + Eq + Hash;
    type V: 'static;

    /// Creates a new empty bucket.
    fn empty() -> AtomicUsize;

    /// True if the bucket pointer is null.
    fn is_null(entry: usize) -> bool;

    /// True if the tombstone flag is set.
    fn is_tombstone(entry: usize) -> bool;

    /// True if the resize flag is set.
    fn is_resize(entry: usize) -> bool;

    /// Check if the key of an entry matches a supplied key.
    fn eq<Q, A>(entry: usize, other: &Q) -> bool
    where
        Self::K: Borrow<Q>,
        Q: ?Sized + Eq,
        A: ObjectAllocator<Bucket<Self::K, Self::V, A>>;

    /// Compare-and-swap primitive that acts on a bucket.
    fn cas<F, A>(entry: &AtomicUsize, f: F, gc: &Gc<Bucket<Self::K, Self::V, A>, A>) -> bool
    where
        F: FnOnce(
            usize,
            Option<(*const Self::K, *const Self::V)>,
        ) -> NewEntryState<Self::K, Self::V, A>,
        A: ObjectAllocator<Bucket<Self::K, Self::V, A>>;
}
