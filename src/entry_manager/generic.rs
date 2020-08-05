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

    fn cas<F, A>(entry: &AtomicUsize, f: F, allocator: &A) -> bool
    where
        F: FnOnce(
            usize,
            Option<(*const Self::K, *const Self::V)>,
        ) -> NewEntryState<Self::K, Self::V, A>,
        A: ObjectAllocator<Bucket<Self::K, Self::V, A>>,
    {
        let loaded_entry = entry.load(Ordering::SeqCst);
        let ptr = strip(loaded_entry) as *const Bucket<K, V, A>;
        let data;

        if !ptr.is_null() {
            unsafe {
                let bucket = &*ptr;
                data = Some((&bucket.key as _, &bucket.value as _));
            }
        } else {
            data = None;
        }

        match f(loaded_entry, data) {
            NewEntryState::Keep => true,
            NewEntryState::Empty => {
                if ptr.is_null() {
                    true
                } else {
                    let swapped =
                        entry.compare_and_swap(loaded_entry, 0, Ordering::SeqCst) == loaded_entry;

                    if swapped {
                        unsafe {
                            (&*ptr).sub_ref(allocator);
                        }

                        true
                    } else {
                        false
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
                    unsafe {
                        (&*ptr).sub_ref(allocator);
                    }
                }

                return swapped;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GenericEntryManager;
    use crate::entry_manager::{EntryManager, NewEntryState};
    use std::sync::atomic::Ordering;
    use crate::alloc::GlobalObjectAllocator;

    #[test]
    fn set_resize() {
        let allocator = GlobalObjectAllocator;
        let atomic_entry = GenericEntryManager::<(), ()>::empty();
        let mut entry = atomic_entry.load(Ordering::SeqCst);
        assert_eq!(GenericEntryManager::<(), ()>::is_resize(entry), false);
        let cas_success = GenericEntryManager::<(), ()>::cas(&atomic_entry, |_, _| NewEntryState::SetResize, &allocator);
        assert!(cas_success);
        entry = atomic_entry.load(Ordering::SeqCst);
        assert_eq!(GenericEntryManager::<(), ()>::is_resize(entry), true);
    }
}
