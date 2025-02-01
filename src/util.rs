//! This module is full of hackery and dark magic.
//! Either spend a day fixing it and quietly submit a PR or don't mention it to anybody.
use core::{mem, ptr};
use std::{marker::PhantomData, mem::ManuallyDrop};

use lock_api::{RawRwLock, RawRwLockDowngrade, RwLockReadGuard, RwLockWriteGuard};

pub const fn ptr_size_bits() -> usize {
    mem::size_of::<usize>() * 8
}

pub fn map_in_place_2<T, U, F: FnOnce(U, T) -> T>((k, v): (U, &mut T), f: F) {
    unsafe {
        // # Safety
        //
        // If the closure panics, we must abort otherwise we could double drop `T`
        let promote_panic_to_abort = AbortOnPanic;

        ptr::write(v, f(k, ptr::read(v)));

        // If we made it here, the calling thread could have already have panicked, in which case
        // We know that the closure did not panic, so don't bother checking.
        std::mem::forget(promote_panic_to_abort);
    }
}

struct AbortOnPanic;

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort()
        }
    }
}


/// A [`RwLockReadGuard`], without the data
pub(crate) struct RwLockReadGuardDetached<'a, R: RawRwLock> {
    lock: &'a R,
    _marker: PhantomData<R::GuardMarker>,
}

impl<R: RawRwLock> Drop for RwLockReadGuardDetached<'_, R> {
    fn drop(&mut self) {
        // Safety: An RwLockReadGuardDetached always holds a shared lock.
        unsafe {
            self.lock.unlock_shared();
        }
    }
}

/// A [`RwLockWriteGuard`], without the data
pub(crate) struct RwLockWriteGuardDetached<'a, R: RawRwLock> {
    lock: &'a R,
    _marker: PhantomData<R::GuardMarker>,
}

impl<R: RawRwLock> Drop for RwLockWriteGuardDetached<'_, R> {
    fn drop(&mut self) {
        // Safety: An RwLockWriteGuardDetached always holds an exclusive lock.
        unsafe {
            self.lock.unlock_exclusive();
        }
    }
}

impl<'a, R: RawRwLock> RwLockReadGuardDetached<'a, R> {
    /// Separates the data from the [`RwLockReadGuard`]
    ///
    /// # Safety
    ///
    /// The data must not outlive the detached guard
    pub(crate) unsafe fn detach_from<T>(guard: RwLockReadGuard<'a, R, T>) -> (Self, &'a T) {
        let rwlock = RwLockReadGuard::rwlock(&ManuallyDrop::new(guard));

        // Safety: There will be no concurrent writes as we are "forgetting" the existing guard,
        // with the safety assumption that the caller will not drop the new detached guard early.
        let data = unsafe { &*rwlock.data_ptr() };
        let guard = RwLockReadGuardDetached {
            // Safety: We are imitating the original RwLockReadGuard. It's the callers
            // responsibility to not drop the guard early.
            lock: unsafe { rwlock.raw() },
            _marker: PhantomData,
        };
        (guard, data)
    }
}

impl<'a, R: RawRwLock> RwLockWriteGuardDetached<'a, R> {
    /// Separates the data from the [`RwLockWriteGuard`]
    ///
    /// # Safety
    ///
    /// The data must not outlive the detached guard
    pub(crate) unsafe fn detach_from<T>(guard: RwLockWriteGuard<'a, R, T>) -> (Self, &'a mut T) {
        let rwlock = RwLockWriteGuard::rwlock(&ManuallyDrop::new(guard));

        // Safety: There will be no concurrent reads/writes as we are "forgetting" the existing guard,
        // with the safety assumption that the caller will not drop the new detached guard early.
        let data = unsafe { &mut *rwlock.data_ptr() };
        let guard = RwLockWriteGuardDetached {
            // Safety: We are imitating the original RwLockWriteGuard. It's the callers
            // responsibility to not drop the guard early.
            lock: unsafe { rwlock.raw() },
            _marker: PhantomData,
        };
        (guard, data)
    }
}

impl<'a, R: RawRwLockDowngrade> RwLockWriteGuardDetached<'a, R> {
    /// # Safety
    ///
    /// The associated data must not mut mutated after downgrading
    pub(crate) unsafe fn downgrade(self) -> RwLockReadGuardDetached<'a, R> {
        // Safety: An RwLockWriteGuardDetached always holds an exclusive lock.
        unsafe { self.lock.downgrade() }
        RwLockReadGuardDetached {
            lock: self.lock,
            _marker: self._marker,
        }
    }
}
