use std::collections::LinkedList;
use std::default::Default;
use std::fmt;
use std::ops::Range;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};

#[macro_export]
#[cfg(feature = "nightly")]
macro_rules! likely {
    ($b:expr) => {
        std::intrinsics::likely($b)
    };
}

#[macro_export]
#[cfg(not(feature = "nightly"))]
macro_rules! likely {
    ($b:expr) => {
        $b
    };
}

#[macro_export]
#[cfg(feature = "nightly")]
macro_rules! unlikely {
    ($b:expr) => {
        std::intrinsics::unlikely($b)
    };
}

#[macro_export]
#[cfg(not(feature = "nightly"))]
macro_rules! unlikely {
    ($b:expr) => {
        $b
    };
}

#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
#[cfg_attr(target_arch = "x86_64", repr(align(128)))]
#[cfg_attr(not(target_arch = "x86_64"), repr(align(64)))]
pub struct CachePadded<T> {
    value: T,
}

unsafe impl<T: Send> Send for CachePadded<T> {}
unsafe impl<T: Sync> Sync for CachePadded<T> {}

impl<T> CachePadded<T> {
    pub fn new(t: T) -> CachePadded<T> {
        CachePadded::<T> { value: t }
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T: fmt::Debug> fmt::Debug for CachePadded<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CachePadded")
            .field("value", &self.value)
            .finish()
    }
}

impl<T> From<T> for CachePadded<T> {
    fn from(t: T) -> Self {
        CachePadded::new(t)
    }
}

const DISCRIMINANT_MASK: usize = std::usize::MAX >> 16;
const POINTER_WIDTH: usize = 48;

pub fn tag_strip(pointer: usize) -> usize {
    pointer & DISCRIMINANT_MASK
}

fn tag_discriminant(pointer: usize) -> Discriminant {
    Discriminant {
        a: (pointer >> POINTER_WIDTH) as u16,
    }
}

fn store_discriminant(pointer: usize, discriminant: Discriminant) -> usize {
    unsafe {
        let discriminant = discriminant.a as usize;
        pointer | ((discriminant) << POINTER_WIDTH)
    }
}

pub fn get_tag_type(pointer: usize) -> PtrTag {
    unsafe { std::mem::transmute(tag_discriminant(pointer).b[0]) }
}

pub fn set_tag_type(pointer: usize, tag: PtrTag) -> usize {
    unsafe {
        let mut d = tag_discriminant(pointer);
        d.b[0] = tag as u8;
        store_discriminant(pointer, d)
    }
}

pub fn get_cache(pointer: usize) -> u8 {
    unsafe { std::mem::transmute(tag_discriminant(pointer).b[1]) }
}

pub fn set_cache(pointer: usize, cache: u8) -> usize {
    unsafe {
        let mut d = tag_discriminant(pointer);
        d.b[1] = cache;
        store_discriminant(pointer, d)
    }
}

#[derive(PartialEq, Eq)]
#[repr(u8)]
pub enum PtrTag {
    None = 0,
    Tombstone = 1,
    Resize = 2,
}

union Discriminant {
    a: u16,
    b: [u8; 2],
}

pub fn derive_filter(x: u64) -> u8 {
    x as u8
}

pub fn range_split(range: Range<usize>, chunk_size: usize) -> LinkedList<Range<usize>> {
    let mut ranges = LinkedList::new();
    let mut next = range.start;
    while next < range.end {
        let mut chunk = next..next + chunk_size;
        if chunk.end > range.end {
            chunk.end = range.end;
        }
        next = chunk.end;
        ranges.push_back(chunk);
    }
    ranges
}

pub struct CircularRange {
    end: usize,
    next: usize,
    step: usize,
}

impl CircularRange {
    pub fn new(_: usize, end: usize, next: usize) -> Self {
        Self { end, next, step: 0 }
    }
}

impl Iterator for CircularRange {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.next;
        self.step += 1;
        self.next += self.step * self.step;
        if unlikely!(self.next >= self.end) {
            self.next &= self.end - 1;
        }
        Some(r)
    }
}

pub fn unreachable() -> ! {
    unreachable!()
}

pub struct FastCounter {
    inner: CachePadded<AtomicUsize>,
}

impl FastCounter {
    pub fn new() -> Self {
        Self {
            inner: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    pub fn increment(&self) {
        self.inner.fetch_add(1, Ordering::AcqRel);
    }

    pub fn decrement(&self) {
        self.inner.fetch_sub(1, Ordering::AcqRel);
    }

    pub fn read(&self) -> usize {
        self.inner.load(Ordering::Relaxed)
    }
}
