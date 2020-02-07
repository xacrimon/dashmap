use once_cell::unsync::Lazy;
use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use crossbeam_epoch::{Pointer, Shared, Guard};
use std::ptr;
use std::ops::{Deref, DerefMut};

pub struct Sanic<T> {
    ptr: *mut T,
}

impl<T> Sanic<T> {
    pub fn new(v: T) -> Self {
        let ptr = local_alloc(Layout::new::<T>()) as _;
        unsafe { ptr::write(ptr, v); }
        Self { ptr }
    }

    pub fn into_shared<'a>(self, _: &'a Guard) -> Shared<'a, T> {
        unsafe { Shared::from_usize(self.ptr as usize) }
    }

    pub unsafe fn from_shared<'a>(s: Shared<'a, T>) -> Self {
        Self { ptr: s.into_usize() as _ }
    }
}

impl<T> Deref for Sanic<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for Sanic<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

impl<T> Drop for Sanic<T> {
    fn drop(&mut self) {
        unsafe { ptr::drop_in_place(self.ptr); }
        local_dealloc(self.ptr as _, Layout::new::<T>());
    }
}

impl<T> Pointer<T> for Sanic<T> {
    fn into_usize(self) -> usize {
        self.ptr as usize
    }

    unsafe fn from_usize(data: usize) -> Self {
        Self { ptr: data as _ }
    }
}

const SEGMENT_SIZE: usize = 64;
const GLOBAL_THRESHOLD: usize = SEGMENT_SIZE / 2;
const COUNTER_SIZE: usize = mem::size_of::<AtomicUsize>();
const SEGMENT_USABLE: usize = SEGMENT_SIZE - COUNTER_SIZE;

thread_local! {
    static CURRENT_SEGMENT: Lazy<RefCell<Segment>> = Lazy::new(|| RefCell::new(Segment::new()));
}

fn local_alloc(layout: Layout) -> *mut u8 {
    if layout.size() > GLOBAL_THRESHOLD {
        return unsafe { alloc(layout) };
    }

    CURRENT_SEGMENT.with(|l| {
        let mut r = l.borrow_mut();

        loop {
            if let Some(ptr) = r.alloc(layout) {
                break ptr;
            } else {
                *r = Segment::new();
            }
        }
    })
}

fn local_dealloc(ptr: *mut u8, layout: Layout) {
    unsafe {
        if layout.size() > GLOBAL_THRESHOLD {
            return dealloc(ptr, layout);
        }

        let mut segment_ptr = ptr as usize;
        while segment_ptr % SEGMENT_SIZE != 0 {
            segment_ptr -= 1;
        }

        let segment_ptr = segment_ptr as *mut Segment;
        (&*segment_ptr).decr();
    }
}

fn segment_layout() -> Layout {
    Layout::from_size_align(SEGMENT_SIZE, SEGMENT_SIZE).unwrap()
}

struct Segment {
    base: *mut AtomicUsize,
    next: usize,
    top: usize,
}

impl Segment {
    fn new() -> Self {
        let base = unsafe { alloc(segment_layout()) } as usize;
        let next = base + COUNTER_SIZE;
        let top = next + SEGMENT_USABLE;

        {
            let c = base as *mut AtomicUsize;
            unsafe {
                *c = AtomicUsize::new(0);
            }
        }

        Self {
            base: base as _,
            next,
            top,
        }
    }

    fn alloc(&mut self, layout: Layout) -> Option<*mut u8> {
        let mut next_aligned = self.next;

        while next_aligned % layout.align() != 0 {
            next_aligned += 1;
        }

        if next_aligned + layout.size() > self.top {
            return None;
        } else {
            self.next = next_aligned + layout.size();
            self.incr();
            return Some(next_aligned as _);
        }
    }

    fn incr(&self) {
        unsafe {
            (&*self.base).fetch_add(1, Ordering::SeqCst);
        }
    }

    fn decr(&self) {
        unsafe {
            if (&*self.base).fetch_sub(1, Ordering::SeqCst) == 1 {
                self.dealloc();
            }
        }
    }

    fn dealloc(&self) {
        unsafe {
            dealloc(self.base as _, segment_layout());
        }
    }
}
