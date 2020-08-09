use super::priority_queue::PriorityQueue;
use crate::utils::shim::sync::Mutex;
use once_cell::sync::Lazy;

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
        Self(ID_ALLOCATOR.lock().unwrap().allocate())
    }
}

impl Drop for ThreadId {
    fn drop(&mut self) {
        ID_ALLOCATOR.lock().unwrap().deallocate(self.0);
    }
}

thread_local! {
    static THREAD_ID: ThreadId = ThreadId::new();
}

pub fn get() -> u32 {
    THREAD_ID.with(|data| data.0)
}
