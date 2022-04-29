use lock_api::GuardSend;
use std::sync::atomic::{AtomicU32, Ordering};

const EXCLUSIVE_BIT: u32 = 1 << 31;

pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;

pub struct RawRwLock {
    data: AtomicU32,
}

unsafe impl lock_api::RawRwLock for RawRwLock {
    type GuardMarker = GuardSend;

    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = RawRwLock {
        data: AtomicU32::new(0),
    };

    fn lock_shared(&self) {
        while !self.try_lock_shared() {}
    }

    fn try_lock_shared(&self) -> bool {
        let x = self.data.load(Ordering::SeqCst);
        if x & EXCLUSIVE_BIT != 0 {
            return false;
        }

        let y = x + 1;
        self.data
            .compare_exchange(x, y, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    unsafe fn unlock_shared(&self) {
        self.data.fetch_sub(1, Ordering::SeqCst);
    }

    fn lock_exclusive(&self) {
        while !self.try_lock_exclusive() {}
    }

    fn try_lock_exclusive(&self) -> bool {
        self.data
            .compare_exchange(0, EXCLUSIVE_BIT, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    unsafe fn unlock_exclusive(&self) {
        self.data.store(0, Ordering::SeqCst)
    }

    fn is_locked(&self) -> bool {
        self.data.load(Ordering::SeqCst) != 0
    }

    fn is_locked_exclusive(&self) -> bool {
        self.data.load(Ordering::SeqCst) & EXCLUSIVE_BIT != 0
    }
}

unsafe impl lock_api::RawRwLockDowngrade for RawRwLock {
    unsafe fn downgrade(&self) {
        self.data.store(1, Ordering::SeqCst);
    }
}
