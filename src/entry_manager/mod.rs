mod generic;

use crate::alloc::ObjectAllocator;
use crate::bucket::Bucket;
use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;

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
    fn eq<Q>(entry: usize, other: &Q, other_hash: u64) -> bool
    where
        Self::K: Borrow<Q>,
        Q: ?Sized + Eq;

    /// Compare-and-swap primitive that acts on a bucket.
    fn cas<F, A: ObjectAllocator<Bucket<Self::K, Self::V, A>>>(entry: &AtomicUsize, f: F) -> bool
    where
        F: FnOnce(
            usize,
            Option<(*const Self::K, *const Self::V)>,
        ) -> NewEntryState<Self::K, Self::V, A>;
}
