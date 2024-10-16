//! This module is full of hackery and dark magic.
//! Either spend a day fixing it and quietly submit a PR or don't mention it to anybody.
use core::cell::UnsafeCell;
use core::{mem, ptr};
use std::mem::ManuallyDrop;

use lock_api::{RawRwLock as _, RawRwLockDowngrade};

use crate::lock::{RawRwLock, RwLockReadGuard, RwLockWriteGuard};

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

/// A simple wrapper around `T`
///
/// This is to prevent UB when using `HashMap::get_key_value`, because
/// `HashMap` doesn't expose an api to get the key and value, where
/// the value is a `&mut T`.
///
/// See [#10](https://github.com/xacrimon/dashmap/issues/10) for details
///
/// This type is meant to be an implementation detail, but must be exposed due to the `Dashmap::shards`
#[repr(transparent)]
pub struct SharedValue<T> {
    value: UnsafeCell<T>,
}

impl<T: Clone> Clone for SharedValue<T> {
    fn clone(&self) -> Self {
        let inner = self.get().clone();

        Self {
            value: UnsafeCell::new(inner),
        }
    }
}

unsafe impl<T: Send> Send for SharedValue<T> {}

unsafe impl<T: Sync> Sync for SharedValue<T> {}

impl<T> SharedValue<T> {
    /// Create a new `SharedValue<T>`
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
        }
    }

    /// Get a shared reference to `T`
    pub fn get(&self) -> &T {
        unsafe { &*self.value.get() }
    }

    /// Get an unique reference to `T`
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() }
    }

    /// Unwraps the value
    pub fn into_inner(self) -> T {
        self.value.into_inner()
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

/// A RwLockReadGuard, without the data
pub(crate) struct GuardRead<'a>(&'a RawRwLock);

impl<'a> Drop for GuardRead<'a> {
    fn drop(&mut self) {
        unsafe {
            self.0.unlock_shared();
        }
    }
}

/// Separates the data from the RwLock ReadGuard
pub(crate) fn split_read_guard<T>(guard: RwLockReadGuard<'_, T>) -> (GuardRead<'_>, &T) {
    let rwlock = RwLockReadGuard::rwlock(&ManuallyDrop::new(guard));

    let data = unsafe { &*rwlock.data_ptr() };
    let guard = unsafe { GuardRead(rwlock.raw()) };
    (guard, data)
}

/// A RwLockWriteGuard, without the data
pub(crate) struct GuardWrite<'a>(&'a RawRwLock);

impl<'a> Drop for GuardWrite<'a> {
    fn drop(&mut self) {
        unsafe {
            self.0.unlock_exclusive();
        }
    }
}

impl<'a> GuardWrite<'a> {
    pub(crate) unsafe fn downgrade(self) -> GuardRead<'a> {
        self.0.downgrade();
        GuardRead(self.0)
    }
}

/// Separates the data from the RwLock WriteGuard
pub(crate) fn split_write_guard<T>(guard: RwLockWriteGuard<'_, T>) -> (GuardWrite<'_>, &mut T) {
    let rwlock = RwLockWriteGuard::rwlock(&ManuallyDrop::new(guard));

    let data = unsafe { &mut *rwlock.data_ptr() };
    let guard = unsafe { GuardWrite(rwlock.raw()) };
    (guard, data)
}
