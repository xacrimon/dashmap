use std::collections::LinkedList;
use std::default::Default;
use std::fmt;
use std::ops::Range;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
#[cfg_attr(target_arch = "x86_64", repr(align(128)))]
#[cfg_attr(not(target_arch = "x86_64"), repr(align(64)))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    pub fn new(t: T) -> CachePadded<T> {
        CachePadded::<T> { value: t }
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline(always)]
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
    #[inline(always)]
    fn from(t: T) -> Self {
        CachePadded::new(t)
    }
}

const DISCRIMINANT_MASK: usize = std::usize::MAX >> 16;
const POINTER_WIDTH: usize = 48;

/// Strip the tag from a pointer, making it safe to deref assuming it is valid.
#[inline(always)]
pub fn tag_strip(pointer: usize) -> usize {
    pointer & DISCRIMINANT_MASK
}

/// Get the full tag of a pointer.
#[inline(always)]
fn tag_discriminant(pointer: usize) -> Discriminant {
    Discriminant {
        a: (pointer >> POINTER_WIDTH) as u16,
    }
}

/// Store the complete 16 bit discriminant in a pointer.
#[inline(always)]
fn store_discriminant(pointer: usize, discriminant: Discriminant) -> usize {
    // # Safety
    // This is safe, we are just reading an union here.
    unsafe {
        let discriminant = discriminant.a as usize;
        pointer | ((discriminant) << POINTER_WIDTH)
    }
}

/// Get the tag type of a tagged pointer.
#[inline(always)]
pub fn get_tag_type(pointer: usize) -> PtrTag {
    // # Safety
    // Completely safe, just union bitfiddling.
    unsafe { std::mem::transmute(tag_discriminant(pointer).b[0]) }
}

/// Set the tag type of a pointer.
#[inline(always)]
pub fn set_tag_type(pointer: usize, tag: PtrTag) -> usize {
    // # Safety
    // This is safe, we're just doing union bitfiddling.
    unsafe {
        let mut d = tag_discriminant(pointer);
        d.b[0] = tag as u8;
        store_discriminant(pointer, d)
    }
}

/// Get the filter bytes from a tagged pointer.
#[inline(always)]
pub fn get_cache(pointer: usize) -> u8 {
    // # Safety
    // This is safe, we're just reading a few off the high bits and using unions to do that.
    unsafe { tag_discriminant(pointer).b[1] }
}

/// Set the cached filter bytes of a pointer.
#[inline(always)]
pub fn set_cache(pointer: usize, cache: u8) -> usize {
    // # Safety
    // This completely safe but uses unions to do a transmutation to a bit more
    // clear. This may possibly be replaced with bitshifting.
    unsafe {
        let mut d = tag_discriminant(pointer);
        d.b[1] = cache;
        store_discriminant(pointer, d)
    }
}

/// Represents a couple of states a pointer inside a table may have.
#[derive(PartialEq, Eq)]
#[repr(u8)]
pub enum PtrTag {
    None = 0,
    Tombstone = 1,
    Resize = 2,
}

/// The full pointer tag.
union Discriminant {
    a: u16,
    b: [u8; 2],
}

/// Derive filter bytes from a hash.
/// This is used as a cheap approxite filter to determine if
/// we should probe a bucket or not.
#[inline(always)]
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

/// A circular iterator, we use this to search the table.
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
        if self.next >= self.end {
            self.next &= self.end - 1;
        }
        Some(r)
    }
}

pub fn unreachable() -> ! {
    unreachable!()
}

/// A fast approximate counter.
/// The current implementation is very naive and optimizing this should be looked into.
pub struct FastCounter {
    inner: CachePadded<AtomicUsize>,
}

impl FastCounter {
    pub fn new() -> Self {
        Self {
            inner: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    #[inline(always)]
    pub fn increment(&self) {
        self.inner.fetch_add(1, Ordering::AcqRel);
    }

    #[inline(always)]
    pub fn decrement(&self) {
        self.inner.fetch_sub(1, Ordering::AcqRel);
    }

    #[inline(always)]
    pub fn read(&self) -> usize {
        self.inner.load(Ordering::Relaxed)
    }
}
