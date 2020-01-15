//! This module is full of hackery and dark magic.
//! Either spend a day fixing it and quietly submit a PR or don't mention it to anybody.

use std::{mem, ptr};
use std::cell::UnsafeCell;

#[inline(always)]
pub const fn ptr_size_bits() -> usize {
    mem::size_of::<usize>() * 8
}

#[inline(always)]
pub fn map_in_place_2<T, U, F: FnOnce(U, T) -> T>((k, v): (U, &mut T), f: F) {
    unsafe {
        // # Safety
        // 
        // If the closure panics, we must abort otherwise we could double drop `T`
        let _promote_panic_to_abort = AbortOnPanic;

        ptr::write(v, f(k, ptr::read(v)));
    }
}

/// # Safety
///
/// Requires that you ensure the reference does not become invalid.
/// The object has to outlive the reference.
#[inline(always)]
pub unsafe fn change_lifetime_const<'a, 'b, T>(x: &'a T) -> &'b T {
    &*(x as *const T)
}

/// # Safety
///
/// Requires that you ensure the reference does not become invalid.
/// The object has to outlive the reference.
#[inline(always)]
pub unsafe fn change_lifetime_mut<'a, 'b, T>(x: &'a mut T) -> &'b mut T {
    &mut *(x as *mut T)
}

pub struct SharedValue<T> {
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for SharedValue<T> {}
unsafe impl<T: Sync> Sync for SharedValue<T> {}

impl<T> SharedValue<T> {
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
        }
    }

    pub fn get(&self) -> &T {
        unsafe { &*self.value.get() }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub(crate) fn as_ptr(&self) -> *mut T {
        self.value.get()
    }
}

pub struct AbortOnPanic;

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort()
        }
    }
}