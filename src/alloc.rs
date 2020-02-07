use std::cell::RefCell;
use std::sync::atomic::{Ordering, AtomicUsize};
use std::alloc::{alloc, dealloc, Layout};
use std::mem;

const SEGMENT_SIZE: usize = 1024;
const COUNTER_SIZE: usize = mem::size_of::<AtomicUsize>();
const SEGMENT_USABLE: usize = SEGMENT_SIZE - COUNTER_SIZE;

fn segment_layout() -> Layout {
    Layout::from_size_align(SEGMENT_SIZE, SEGMENT_SIZE).unwrap()
}

struct Segment {
    base: *mut AtomicUsize,
    next: usize,
    top: usize,
}

impl Segment {
    pub fn new() -> Self {
        let base = unsafe { alloc(segment_layout()) } as usize;
        let next = base + COUNTER_SIZE;
        let top = next + SEGMENT_USABLE;

        {
            let c = base as *mut AtomicUsize;
            unsafe { *c = AtomicUsize::new(0); }
        }

        Self {
            base: base as _,
            next,
            top,
        }
    }

    pub fn incr(&self) {
        unsafe { (&*self.base).fetch_add(1, Ordering::SeqCst); }
    }

    pub fn decr(&self) {
        unsafe {
            if (&*self.base).fetch_sub(1, Ordering::SeqCst) == 1 {
                self.dealloc();
            }
        }
    }

    pub fn dealloc(&self) {
        unsafe { dealloc(self.base as _, segment_layout()); }
    }
}
