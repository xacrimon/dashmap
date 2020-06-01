use std::alloc::{alloc, dealloc, Layout};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct ABox<T> {
    refs: AtomicUsize,
    data: T,
}

#[inline(always)]
pub fn sarc_new<T>(v: T) -> *mut ABox<T> {
    let layout = Layout::new::<ABox<T>>();
    let a = ABox {
        refs: AtomicUsize::new(1),
        data: v,
    };
    let p = local_alloc(layout);

    // # Safety
    // We have allocated some memory directly using the
    // correct layout so this is perfectly fine to do.
    unsafe {
        ptr::write(p as *mut _, a);
    }

    p as _
}

/// Dereference the opaque reference counted pointer.
#[inline(always)]
pub fn sarc_deref<'a, T>(p: *mut ABox<T>) -> &'a T {
    // # Safety
    // This is safe to do as long as the caller has provided a valid
    // pointer. Otherwise the behaviour is undefined.
    unsafe { &(*p).data }
}

/// Increments the reference count.
#[inline(always)]
pub fn sarc_add_copy<T>(p: *mut ABox<T>) {
    // # Safety
    // This is safe to do as long as the caller has provided a valid pointer.
    unsafe {
        (*p).refs.fetch_add(1, Ordering::AcqRel);
    }
}

/// Decrements the reference count.
#[inline(always)]
pub fn sarc_remove_copy<T>(p: *mut ABox<T>) {
    debug_assert!(!p.is_null());

    // # Safety
    // This is safe to do as long as the caller has provided a valid pointer.
    unsafe {
        if (*p).refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            sarc_dealloc(p);
        }
    }
}

/// Deallocate a reference counted pointer and drop the value behind it.
///
/// # Safety
/// This is fine as long as the caller has provided a valid pointer.
#[inline(always)]
unsafe fn sarc_dealloc<T>(p: *mut ABox<T>) {
    ptr::drop_in_place::<T>(sarc_deref(p) as *const _ as *mut _);
    let layout = Layout::new::<ABox<T>>();
    local_dealloc(p as _, layout);
}

/// Allocates some memory based on a layout.
#[inline(always)]
fn local_alloc(layout: Layout) -> *mut u8 {
    // # Safety
    // This is safe but needs an unsafe block because the allocator functions are marked unsafe.
    // Reading data from this pointer without initializing it first is undefined behaviour.
    unsafe { alloc(layout) }
}

/// Deallocates a some memory based on a layout.
///
/// # Safety
/// This is safe as long as the called has provided a valid pointer
/// that was previously allocated by `local_alloc`.
/// Otherwise the behaviour is undefined.
#[inline(always)]
unsafe fn local_dealloc(ptr: *mut u8, layout: Layout) {
    dealloc(ptr, layout);
}
