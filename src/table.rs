use crate::util::{
    clear_bit, read_bit, set_bit, u64_read_byte, u64_write_byte, RESIZE_BIT, TOMBSTONE_BIT,
};
use std::sync::atomic::{Ordering, AtomicPtr, AtomicU64, AtomicU32};
use std::ptr;

struct Group<T> {
    cache: AtomicU64,
    nodes: [AtomicPtr<T>; 8],
}

impl<T> Group<T> {
    fn new() -> Self {
        Self {
            cache: AtomicU64::new(0),
            nodes: [
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
            ]
        }
    }

    fn probe(&self, filter: u8, mut apply: impl FnMut(*mut T)) {
        let cache = self.cache.load(Ordering::SeqCst);
        for i in 0..8 {
            if u64_read_byte(cache, i) == filter {
                let pointer = self.nodes[i].load(Ordering::SeqCst);
                apply(pointer);
            }
        }
    }

    fn publish(&self, i: usize, cache_maybe_current: u8, pointer_maybe_current: *mut T, cache_new: u8, pointer_new: *mut T) -> bool {
        let cache_all_current = self.cache.load(Ordering::SeqCst);
        let cache_current_sq = u64_write_byte(cache_all_current, i, cache_maybe_current);
        let updated_all_cache = u64_write_byte(cache_all_current, i, cache_new);
        if self.cache.compare_and_swap(cache_current_sq, updated_all_cache, Ordering::SeqCst) != cache_current_sq { return false; }
        if self.nodes[i].compare_and_swap(pointer_maybe_current, pointer_new, Ordering::SeqCst) != pointer_maybe_current { return false; }
        true
    }
}
