use lock_api::GuardSend;

pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;

pub struct RawRwLock {}

unsafe impl lock_api::RawRwLock for RawRwLock {
    type GuardMarker = GuardSend;

    const INIT: Self = RawRwLock {};

    fn lock_shared(&self) {
        todo!()
    }

    fn try_lock_shared(&self) -> bool {
        todo!()
    }

    unsafe fn unlock_shared(&self) {
        todo!()
    }

    fn lock_exclusive(&self) {
        todo!()
    }

    fn try_lock_exclusive(&self) -> bool {
        todo!()
    }

    unsafe fn unlock_exclusive(&self) {
        todo!()
    }

    fn is_locked(&self) -> bool {
        todo!()
    }

    fn is_locked_exclusive(&self) -> bool {
        todo!()
    }
}

unsafe impl lock_api::RawRwLockDowngrade for RawRwLock {
    unsafe fn downgrade(&self) {
        todo!()
    }
}
