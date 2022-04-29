use lock_api::GuardSend;
use std::hint;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};

const USIZE_BITS: usize = mem::size_of::<usize>() * 8;
const EXCLUSIVE_BIT: usize = 1 << (USIZE_BITS - 1);

pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;

pub struct RawRwLock {
    data: AtomicUsize,
}

unsafe impl lock_api::RawRwLock for RawRwLock {
    type GuardMarker = GuardSend;

    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = RawRwLock {
        data: AtomicUsize::new(0),
    };

    fn lock_shared(&self) {
        while !self.try_lock_shared() {
            hint::spin_loop();
        }
    }

    fn try_lock_shared(&self) -> bool {
        let x = self.data.load(Ordering::Acquire);
        if x & EXCLUSIVE_BIT != 0 {
            return false;
        }

        let y = x + 1;
        self.data
            .compare_exchange(x, y, Ordering::Release, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock_shared(&self) {
        self.data.fetch_sub(1, Ordering::Release);
    }

    fn lock_exclusive(&self) {
        while !self.try_lock_exclusive() {
            hint::spin_loop();
        }
    }

    fn try_lock_exclusive(&self) -> bool {
        self.data
            .compare_exchange(0, EXCLUSIVE_BIT, Ordering::Release, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock_exclusive(&self) {
        self.data.store(0, Ordering::Release)
    }

    fn is_locked(&self) -> bool {
        self.data.load(Ordering::Acquire) != 0
    }

    fn is_locked_exclusive(&self) -> bool {
        self.data.load(Ordering::Acquire) & EXCLUSIVE_BIT != 0
    }
}

unsafe impl lock_api::RawRwLockDowngrade for RawRwLock {
    unsafe fn downgrade(&self) {
        self.data.store(1, Ordering::SeqCst);
    }
}
