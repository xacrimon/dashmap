use core::default::Default;
use core::fmt;
use core::ops::{Deref, DerefMut};

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

pub const TOMBSTONE_BIT: usize = 48;
pub const RESIZE_BIT: usize = 49;

pub fn set_bit(x: usize, b: usize) -> usize {
    x | 1 << b
}

pub fn clear_bit(x: usize, b: usize) -> usize {
    x & !(1 << b)
}

pub fn read_bit(x: usize, b: usize) -> bool {
    ((x >> b) & 1) != 0
}
