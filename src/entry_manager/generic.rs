use super::{EntryManager, NewEntryState};
use crate::alloc::ObjectAllocator;
use crate::bucket::Bucket;
use crate::gc::Gc;
use crate::shim::sync::atomic::{AtomicUsize, Ordering};
use std::borrow::Borrow;
use std::hash::Hash;
use std::marker::PhantomData;

fn strip(x: usize) -> usize {
    x & !(1 << 0 | 1 << 1)
}

fn is_on(field: usize, idx: usize) -> bool {
    field & (1 << idx) != 0
}

fn set(field: usize, idx: usize) -> usize {
    field | 1 << idx
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

    fn cas<F, A>(entry: &AtomicUsize, f: F, gc: &Gc<Bucket<Self::K, Self::V, A>, A>) -> bool
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
                    let tombstone = set(0, 0);
                    let swapped = entry.compare_and_swap(loaded_entry, tombstone, Ordering::SeqCst)
                        == loaded_entry;

                    if swapped {
                        let tag = unsafe { (&*ptr).tag };
                        gc.retire(tag);

                        true
                    } else {
                        false
                    }
                }
            }

            NewEntryState::SetResize => {
                let new = set(loaded_entry, 1);
                entry.compare_and_swap(loaded_entry, new, Ordering::SeqCst) == loaded_entry
            }

            NewEntryState::New(element) => {
                let packed = element as usize;
                let swapped =
                    entry.compare_and_swap(loaded_entry, packed, Ordering::SeqCst) == loaded_entry;

                if swapped && !ptr.is_null() {
                    let tag = unsafe { (&*ptr).tag };
                    gc.retire(tag);
                }

                return swapped;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GenericEntryManager;
    use crate::alloc::{GlobalObjectAllocator, ObjectAllocator};
    use crate::bucket::Bucket;
    use crate::entry_manager::{EntryManager, NewEntryState};
    use crate::gc::Gc;
    use crate::shim::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn set_resize() {
        let gc = Gc::new(GlobalObjectAllocator);
        let atomic_entry = GenericEntryManager::<(), ()>::empty();
        let mut entry = atomic_entry.load(Ordering::SeqCst);
        assert!(!GenericEntryManager::<(), ()>::is_resize(entry));

        let cas_success =
            GenericEntryManager::<(), ()>::cas(&atomic_entry, |_, _| NewEntryState::SetResize, &gc);

        assert!(cas_success);
        entry = atomic_entry.load(Ordering::SeqCst);
        assert!(GenericEntryManager::<(), ()>::is_resize(entry));
    }

    fn create_occupied(key: i32, value: i32) -> AtomicUsize {
        let gc = Gc::new(GlobalObjectAllocator);
        let atomic_entry = GenericEntryManager::<i32, i32>::empty();

        let cas_success = GenericEntryManager::<i32, i32>::cas(
            &atomic_entry,
            |_, _| {
                let bucket = Bucket::new(key, value);
                let (_, bucket_ptr) = gc.allocator.allocate(bucket);
                NewEntryState::New(bucket_ptr)
            },
            &gc,
        );

        assert!(cas_success);
        return atomic_entry;
    }

    #[test]
    fn create_check_occupied() {
        let key = 5;
        let value = 7;

        let gc = Gc::new(GlobalObjectAllocator);
        let atomic_entry = create_occupied(key, value);
        let mut is_eq = false;

        let cas_success = GenericEntryManager::<i32, i32>::cas(
            &atomic_entry,
            |_, data| {
                let (kptr, vptr) = data.unwrap();
                unsafe {
                    is_eq = *kptr == key && *vptr == value;
                }
                NewEntryState::Keep
            },
            &gc,
        );

        assert!(cas_success);
        assert!(is_eq);
    }

    #[test]
    fn tombstone_check() {
        let key = -52;
        let value = 1298;

        let gc = Gc::new(GlobalObjectAllocator);
        let atomic_entry = create_occupied(key, value);

        let cas_success =
            GenericEntryManager::<i32, i32>::cas(&atomic_entry, |_, _| NewEntryState::Empty, &gc);

        assert!(cas_success);
        let entry = atomic_entry.load(Ordering::SeqCst);
        assert!(GenericEntryManager::<i32, i32>::is_tombstone(entry));
    }
}
