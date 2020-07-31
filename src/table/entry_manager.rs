use crate::alloc::ABox;
use crate::element::Element;
use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;

pub trait EntryManager {
    type K: 'static + Eq + Hash;
    type V: 'static;

    fn empty() -> AtomicUsize;
    fn is_null(entry: usize) -> bool;
    fn is_tombstone(entry: usize) -> bool;
    fn is_resize(entry: usize) -> bool;
    fn cas<F>(entry: &AtomicUsize, f: F) -> bool
    where
        F: FnOnce(
            usize,
            Option<(*const Self::K, *const Self::V)>,
        ) -> NewEntryState<Self::K, Self::V>;

    fn eq<Q>(entry: usize, other: &Q, other_hash: u64) -> bool
    where
        Self::K: Borrow<Q>,
        Q: ?Sized + Eq;
}

pub enum NewEntryState<K: 'static + Eq + Hash, V: 'static> {
    Empty,
    Keep,
    SetResize,
    New(*mut ABox<Element<K, V>>),
}
