use super::entry_manager::{EntryManager, NewEntryState};
use crate::alloc::{sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::Element;
use std::borrow::Borrow;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

fn strip(x: usize) -> usize {
    x & !0b11
}

pub struct GenericEntryManager<K, V> {
    _marker: PhantomData<(K, V)>,
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
        F: FnOnce(
            usize,
            Option<(*const Self::K, *const Self::V)>,
        ) -> NewEntryState<Self::K, Self::V>,
    {
        let loaded_entry = entry.load(Ordering::SeqCst);
        let ptr = strip(loaded_entry) as *const ABox<Element<K, V>>;
        let data;

        if !ptr.is_null() {
            unsafe {
                let element = sarc_deref(ptr);
                data = Some((&element.key as _, &element.value as _));
            }
        } else {
            data = None;
        }

        match f(loaded_entry, data) {
            NewEntryState::Keep => true,
            NewEntryState::Empty => {
                if ptr.is_null() {
                    return true;
                } else {
                    let swapped =
                        entry.compare_and_swap(loaded_entry, 0, Ordering::SeqCst) == loaded_entry;

                    if swapped {
                        sarc_remove_copy(ptr as *mut ABox<Element<K, V>>);
                        return true;
                    } else {
                        return false;
                    }
                }
            }
            NewEntryState::SetResize => {
                let new = loaded_entry | 0b01;
                entry.compare_and_swap(loaded_entry, new, Ordering::SeqCst) == loaded_entry
            }
            NewEntryState::New(element) => {
                let packed = element as usize;
                let swapped =
                    entry.compare_and_swap(loaded_entry, packed, Ordering::SeqCst) == loaded_entry;

                if swapped && !ptr.is_null() {
                    sarc_remove_copy(ptr as *mut ABox<Element<K, V>>);
                }

                return swapped;
            }
        }
    }

    fn eq<Q>(entry: usize, other: &Q, other_hash: u64) -> bool
    where
        Self::K: Borrow<Q>,
        Q: ?Sized + Eq,
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
