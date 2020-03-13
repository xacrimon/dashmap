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

    pub fn into_inner(self) -> T {
        self.value
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

const TOMBSTONE_BIT: usize = 0;
const RESIZE_BIT: usize = 1;

pub enum PtrTag {
    None,
    Tombstone,
    Resize,
}

pub fn set_tag<T>(ptr: *mut T, tag: PtrTag) -> *mut T {
    let ptr = ptr as usize;
    (match tag {
        PtrTag::None => clear_bit(clear_bit(ptr, TOMBSTONE_BIT), RESIZE_BIT),
        PtrTag::Tombstone => clear_bit(set_bit(ptr, TOMBSTONE_BIT), RESIZE_BIT),
        PtrTag::Resize => set_bit(clear_bit(ptr, TOMBSTONE_BIT), RESIZE_BIT),
    }) as *mut T
}

pub fn get_tag<T>(ptr: *mut T) -> PtrTag {
    let ptr = ptr as usize;
    if read_bit(ptr, TOMBSTONE_BIT) {
        PtrTag::Tombstone
    } else if read_bit(ptr, RESIZE_BIT) {
        PtrTag::Resize
    } else {
        PtrTag::None
    }
}

fn set_bit(x: usize, b: usize) -> usize {
    x | 1 << b
}

fn clear_bit(x: usize, b: usize) -> usize {
    x & !(1 << b)
}

fn read_bit(x: usize, b: usize) -> bool {
    ((x >> b) & 1) != 0
}

pub fn u64_read_byte(x: u64, n: usize) -> u8 {
    debug_assert!(n < 8);
    unsafe { *x.to_ne_bytes().get_unchecked(n) }
}

pub fn u64_write_byte(x: u64, n: usize, b: u8) -> u64 {
    debug_assert!(n < 8);
    let mut a = x.to_ne_bytes();
    unsafe {
        *a.get_unchecked_mut(n) = b;
    }
    u64::from_ne_bytes(a)
}

pub fn derive_filter(x: u64) -> u8 {
    let a = x.to_ne_bytes();
    a[0] ^ a[1] ^ a[2] ^ a[3] ^ a[4] ^ a[5] ^ a[6] ^ a[7]
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
    start: usize,
    end: usize,
    next: usize,
}

impl CircularRange {
    pub fn new(start: usize, end: usize, next: usize) -> Self {
        Self { start, end, next }
    }
}

impl Iterator for CircularRange {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.next;
        self.next += 1;
        if unlikely!(self.next == self.end) {
            self.next = self.start;
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
        self.inner.fetch_add(1, Ordering::SeqCst);
    }

    pub fn decrement(&self) {
        self.inner.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn read(&self) -> usize {
        self.inner.load(Ordering::SeqCst)
    }
}
