//! This module is full of hackery and dark magic.
//! Either spend a day fixing it and quietly submit a PR or don't mention it to anybody.
use core::mem;
use std::{marker::PhantomData, mem::ManuallyDrop};

use lock_api::{RawRwLock, RawRwLockDowngrade, RwLockReadGuard, RwLockWriteGuard};

pub const fn ptr_size_bits() -> usize {
    mem::size_of::<usize>() * 8
}

pub(crate) fn try_map<F, T, U>(mut t: &mut T, f: F) -> Result<&mut U, &mut T>
where
    F: FnOnce(&mut T) -> Option<&mut U>,
{
    use polonius_the_crab::{polonius, polonius_return};
    polonius!(|t| -> Result<&'polonius mut U, &mut T> {
        if let Some(u) = f(t) {
            polonius_return!(Ok(u));
        }
    });
    Err(t)
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
