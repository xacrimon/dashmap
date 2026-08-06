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
        // Do not drop the write guard - otherwise we will trigger a downgrade + unlock_exclusive,
        // which is incorrect
        let this = ManuallyDrop::new(self);

        // Safety: An RwLockWriteGuardDetached always holds an exclusive lock.
        unsafe { this.lock.downgrade() }
        RwLockReadGuardDetached {
            lock: this.lock,
            _marker: this._marker,
        }
    }
}

/// A guard that owns a `Box<[MaybeUninit<T>]>` and tracks how many elements
/// have been initialized. On drop it drops every initialized element; the
/// box itself handles deallocation. On success, [`Self::assume_init`] transmute\-s
/// the box into a `Box<[T]>` without dropping anything.
///
/// This avoids an intermediate `Vec` allocation when constructing a
/// `Box<[T]>` element-by-element in a fallible loop.
pub(crate) struct InitSliceGuard<T> {
    slab: Box<[mem::MaybeUninit<T>]>,
    init: usize,
}

impl<T> InitSliceGuard<T> {
    /// Allocates a boxed slice of `len` uninitialized `T` elements.
    ///
    /// Returns `None` if the layout overflows or the allocation fails.
    pub fn new(len: usize) -> Option<Self> {
        let layout = core::alloc::Layout::array::<mem::MaybeUninit<T>>(len).ok()?;
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` is non-null, correctly aligned, and sized for `len`
        // elements of `MaybeUninit<T>`. MaybeUninit has no invalid bitpatterns.
        let slab: Box<[mem::MaybeUninit<T>]> = unsafe {
            Box::from_raw(ptr::slice_from_raw_parts_mut(
                ptr as *mut mem::MaybeUninit<T>,
                len,
            ))
        };
        Some(Self { slab, init: 0 })
    }

    /// Returns a mutable pointer to the slot at index `i`.
    ///
    /// # Panics
    ///
    /// Panics if `i` is out of bounds.
    pub fn get(&mut self, i: usize) -> *mut T {
        self.slab[i].as_mut_ptr()
    }

    /// Marks one more element as initialized.
    pub fn mark_init(&mut self) {
        self.init += 1;
    }

    /// Consumes the guard and returns a `Box<[T]>`.
    ///
    /// # Safety
    ///
    /// All elements must have been initialized (i.e. `mark_init` called
    /// exactly `slab.len()` times). No element may be left uninitialized.
    pub unsafe fn assume_init(mut self) -> Box<[T]> {
        // SAFETY: The caller guarantees all elements are initialized.
        // Box<[MaybeUninit<T>]> and Box<[T]> share the same fat-pointer layout,
        // so casting the thin pointer is valid. We forget `self` to prevent
        // the Drop impl from running (which would double-drop initialized elems).
        let ptr = self.slab.as_mut_ptr().cast::<T>();
        let len = self.slab.len();
        mem::forget(self);
        unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(ptr, len)) }
    }
}

impl<T> Drop for InitSliceGuard<T> {
    fn drop(&mut self) {
        for i in 0..self.init {
            // SAFETY: Elements 0..init have been written via get()/mark_init().
            unsafe {
                ptr::drop_in_place(self.slab.as_mut_ptr().add(i).cast::<T>());
            }
        }
    }
}
