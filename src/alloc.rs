use crossbeam_epoch::{Atomic, Guard, Pointer, Shared};
use once_cell::unsync::Lazy;
use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ABox<T> {
    refs: AtomicUsize,
    data: T,
}

pub struct Sarc<T> {
    ptr: *const ABox<T>,
}

impl<T> Sarc<T> {
    pub fn new(v: T) -> Self {
        unsafe {
            let ptr = local_alloc(Layout::new::<ABox<T>>()) as *mut ABox<T>;
            ptr::write(
                ptr,
                ABox {
                    refs: AtomicUsize::new(0),
                    data: v,
                },
            );
            Self { ptr }
        }
    }

    pub fn into_shared<'a>(self, _: &'a Guard) -> Shared<'a, T> {
        let ptr = unsafe { Shared::from_usize(self.ptr as usize) };
        mem::forget(self);
        ptr
    }

    pub unsafe fn from_shared<'a>(s: Shared<'a, T>) -> Self {
        Self {
            ptr: s.into_usize() as _,
        }
    }

    pub unsafe fn from_shared_maybe<'a>(s: Shared<'a, T>) -> Option<Self> {
        if !s.with_tag(0).is_null() {
            Some(Self {
                ptr: s.into_usize() as _,
            })
        } else {
            None
        }
    }

    pub fn incr(&self) {
        unsafe {
            (*self.ptr).refs.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn decr(&self) -> bool {
        unsafe { (*self.ptr).refs.fetch_sub(1, Ordering::SeqCst) == 1 }
    }

    pub unsafe fn nd_data(self) -> &'static T {
        let r = mem::transmute(&*self);
        mem::forget(self);
        r
    }
}

unsafe impl<T: Send> Send for Sarc<T> {}
unsafe impl<T: Sync> Sync for Sarc<T> {}

impl<T> Clone for Sarc<T> {
    fn clone(&self) -> Self {
        unsafe {
            (&*self.ptr).refs.fetch_add(1, Ordering::SeqCst);
        }

        Self { ptr: self.ptr }
    }
}

impl<T> Drop for Sarc<T> {
    fn drop(&mut self) {
        unsafe {
            if (&*self.ptr).refs.fetch_sub(1, Ordering::SeqCst) == 1 {
                ptr::read(&(&*self.ptr).data);
                local_dealloc(self.ptr as _, Layout::new::<ABox<T>>());
            }
        }
    }
}

impl<T> Deref for Sarc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &(&*self.ptr).data }
    }
}

impl<T> Pointer<T> for Sarc<T> {
    fn into_usize(self) -> usize {
        let u = self.ptr as usize;
        mem::forget(self);
        u
    }

    unsafe fn from_usize(data: usize) -> Self {
        Self { ptr: data as _ }
    }
}

pub struct Sanic<T> {
    ptr: *mut T,
}

impl<T> Sanic<T> {
    pub fn new(v: T) -> Self {
        let ptr = local_alloc(Layout::new::<T>()) as _;
        unsafe {
            ptr::write(ptr, v);
        }
        debug_assert_ne!(ptr as usize, 0);
        //dbg!(ptr as usize);
        Self { ptr }
    }

    pub fn atomic(v: T) -> Atomic<T> {
        let atomic = Atomic::null();
        atomic.store(Self::new(v), Ordering::Relaxed);
        atomic
    }

    pub fn into_shared<'a>(self, _: &'a Guard) -> Shared<'a, T> {
        let ptr = unsafe { Shared::from_usize(self.ptr as usize) };
        mem::forget(self);
        ptr
    }

    pub unsafe fn from_shared<'a>(s: Shared<'a, T>) -> Self {
        Self {
            ptr: s.into_usize() as _,
        }
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
        unsafe {
            ptr::drop_in_place(self.ptr);
        }
        local_dealloc(self.ptr as _, Layout::new::<T>());
    }
}

impl<T> Pointer<T> for Sanic<T> {
    fn into_usize(self) -> usize {
        let u = self.ptr as usize;
        mem::forget(self);
        u
    }

    unsafe fn from_usize(data: usize) -> Self {
        Self { ptr: data as _ }
    }
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

fn local_dealloc(ptr: *mut u8, layout: Layout) {
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
    assert!(align.is_power_of_two(), "`align` must be a power of two");
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
