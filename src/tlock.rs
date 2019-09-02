use parking_lot::RwLock;
use std::mem;
use std::sync::atomic::{AtomicU32, Ordering};

// TO-DO: fix persisent consistency with proper locking and maintaining them for things like iterators and refs

fn thread_id() -> u32 {
    let id = std::thread::current().id();
    fxhash::hash32(&id)
}

pub struct TransactionLock {
    lock: RwLock<()>,
    unique_accessor: AtomicU32,
}

impl TransactionLock {
    pub fn new() -> Self {
        Self {
            lock: RwLock::new(()),
            unique_accessor: AtomicU32::new(0),
        }
    }

    pub fn acquire_unique(&self) {
        mem::forget(self.lock.write());
        self.unique_accessor.store(thread_id(), Ordering::SeqCst);
    }

    pub unsafe fn release_unique(&self) {
        self.lock.force_unlock_write();
        self.unique_accessor.store(0, Ordering::SeqCst);
    }

    pub fn acquire_shared(&self) {
        loop {
            if let Some(guard) = self.lock.try_read() {
                mem::forget(guard);
                return;
            }

            if thread_id() == self.unique_accessor.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    pub unsafe fn release_shared(&self) {
        if thread_id() != self.unique_accessor.load(Ordering::SeqCst) {
            self.lock.force_unlock_read();
        }
    }
}
