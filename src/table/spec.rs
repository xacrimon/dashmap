use super::entry_manager::{EntryManager, NewEntryState};
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::atomic::{Ordering, AtomicUsize};
use std::borrow::Borrow;
use crate::element::Element;
use crate::alloc::{ABox, sarc_deref};

fn strip(x: usize) -> usize {
    #[cfg(target_pointer_width = "64")]
    const MASK: usize = 0b0011111111111111111111111111111111111111111111111111111111111111;

    #[cfg(target_pointer_width = "32")]
    const MASK: usize = 0b00111111111111111111111111111111;

    x & MASK
}

pub struct GenericEntryManager<K, V> {
    _marker: PhantomData<(K, V)>
}

impl<K: 'static + Eq + Hash, V: 'static> EntryManager for GenericEntryManager<K, V> {
    type K = K;
    type V = V;

    fn empty() -> AtomicUsize {
        AtomicUsize::new(0)
    }

    fn is_tombstone(entry: usize) -> bool {
        entry & (1 << 0) != 0
    }

    fn is_resize(entry: usize) -> bool {
        entry & (1 << 1) != 0
    }

    fn cas<F>(entry: &AtomicUsize, f: F) -> bool
    where
        F: FnOnce(usize, Option<(*const Self::K, *const Self::V)>) -> NewEntryState<Self::K, Self::V>
    {
        todo!()
    }

    fn eq<Q>(entry: usize, other: &Q, other_hash: u64) -> bool
    where
        Self::K: Borrow<Q>,
        Q: ?Sized + Eq
    {
        if entry == 0 || Self::is_tombstone(entry) || Self::is_resize(entry) {
            false
        } else {
            let element_ptr = strip(entry) as *const ABox<Element<K, V>>;
            let element = sarc_deref(element_ptr);
            element.hash == other_hash && element.key.borrow() == other
        }
    }
}
