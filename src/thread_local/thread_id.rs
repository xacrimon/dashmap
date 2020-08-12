//! This module deals with allocating thread ids.
//! We aggressively reuse ids and try to keep them as low as possible.
//! This important because low reusable thread ids allows us to use lookup tables
//! instead of hash tables for storing thread local data.

use super::priority_queue::PriorityQueue;
use crate::utils::{hint::UnwrapUnchecked, shim::sync::Mutex};
use once_cell::sync::Lazy;

/// This structure allocates ids.
/// It is compose of a `limit` integer and a list of free ids lesser than `limit`.
/// If an allocation is attempted and the list is empty,
/// we increment limit and return the previous value.
struct IdAllocator {
    limit: u32,
    free: PriorityQueue<u32>,
}

impl IdAllocator {
    fn new() -> Self {
        Self {
            limit: 0,
            free: PriorityQueue::new(),
        }
    }

    fn allocate(&mut self) -> u32 {
        self.free.pop().unwrap_or_else(|| {
            let id = self.limit;
            self.limit += 1;
            id
        })
    }

    fn deallocate(&mut self, id: u32) {
        self.free.push(id);
    }
}

static ID_ALLOCATOR: Lazy<Mutex<IdAllocator>> = Lazy::new(|| Mutex::new(IdAllocator::new()));

struct ThreadId(u32);

impl ThreadId {
    fn new() -> Self {
        unsafe { Self(ID_ALLOCATOR.lock().unwrap_unchecked().allocate()) }
    }
}

/// Drop is implemented here because it's the only clean way to run code when a thread exits.
impl Drop for ThreadId {
    fn drop(&mut self) {
        unsafe {
            ID_ALLOCATOR.lock().unwrap_unchecked().deallocate(self.0);
        }
    }
}

thread_local! {
    static THREAD_ID: ThreadId = ThreadId::new();
}

pub fn get() -> u32 {
    THREAD_ID.with(|data| data.0)
}
