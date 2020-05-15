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
    unsafe {
        ptr::write(p as *mut _, a);
    }
    p as _
}

#[inline(always)]
pub fn sarc_deref<'a, T>(p: *mut ABox<T>) -> &'a T {
    unsafe { &(*p).data }
}

#[inline(always)]
pub fn sarc_add_copy<T>(p: *mut ABox<T>) {
    unsafe {
        (*p).refs.fetch_add(1, Ordering::AcqRel);
    }
}

#[inline(always)]
pub fn sarc_remove_copy<T>(p: *mut ABox<T>) {
    debug_assert!(!p.is_null());

    unsafe {
        if (*p).refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            sarc_dealloc(p);
        }
    }
}

#[inline(always)]
unsafe fn sarc_dealloc<T>(p: *mut ABox<T>) {
    ptr::drop_in_place::<T>(sarc_deref(p) as *const _ as *mut _);
    let layout = Layout::new::<ABox<T>>();
    local_dealloc(p as _, layout);
}

#[inline(always)]
fn local_alloc(layout: Layout) -> *mut u8 {
    unsafe { alloc(layout) }
}

#[inline(always)]
unsafe fn local_dealloc(ptr: *mut u8, layout: Layout) {
    dealloc(ptr, layout);
}
