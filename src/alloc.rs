use crossbeam_epoch::{Atomic, Guard, Pointer, Shared};
use once_cell::unsync::Lazy;
use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct ABox<T> {
    refs: AtomicUsize,
    data: T,
}

pub fn sarc_new<T>(v: T) -> *const ABox<T> {
    let layout = Layout::new::<Abox<T>>();
    let a = ABox {
        refs: AtomicUsize::new(0),
    };
    let p = local_alloc(layout);
    unsafe { ptr::write(p as *mut _, a); }
    p
}

pub fn sarc_deref<'a, T>(p: *const ABox<T>) -> &'a T {
    unsafe { &(*p).data }
}

pub fn sarc_add_copy<T>(p: *const ABox<T>) {
    unsafe {
        (*p).refs.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn sarc_remove_copy<T>(p: *const ABox<T>) {
    unsafe {
        if (*p).refs.fetch_sub(1, Ordering::SeqCst) == 1 {
            sarc_dealloc(p);
        }
    }
}

unsafe fn sarc_dealloc<T>(p: *const ABox<T>) {
    local_dealloc(p as _);
}

const SEGMENT_SIZE: usize = 64 * 1024;
const GLOBAL_THRESHOLD: usize = SEGMENT_SIZE / 2;
const COUNTER_SIZE: usize = mem::size_of::<AtomicUsize>();
const SEGMENT_USABLE: usize = SEGMENT_SIZE - COUNTER_SIZE;

thread_local! {
    static CURRENT_SEGMENT: Lazy<RefCell<Segment>> = Lazy::new(|| RefCell::new(Segment::new()));
}

fn local_alloc(layout: Layout) -> *mut u8 {
    if layout.size() > GLOBAL_THRESHOLD {
        unsafe { alloc(layout) }
    } else {
        let ptr = CURRENT_SEGMENT.with(|l| {
            let mut r = l.borrow_mut();

            loop {
                if let Some(ptr) = r.alloc(layout) {
                    break ptr;
                } else {
                    *r = Segment::new();
                }
            }
        });
        ptr
    }
}

unsafe fn local_dealloc(ptr: *mut u8, layout: Layout) {
    unsafe {
        if layout.size() > GLOBAL_THRESHOLD {
            dealloc(ptr, layout);
        } else {
            let base_ptr = align_down(ptr as usize, SEGMENT_SIZE) as *const AtomicUsize;

            if (&*base_ptr).fetch_sub(1, Ordering::SeqCst) == 1 {
                dealloc(base_ptr as _, segment_layout());
            }
        }
    }
}

fn align_down(addr: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "`align` must be a power of two");
    addr & !(align - 1)
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
}
