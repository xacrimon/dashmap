use std::{mem, ptr};

#[inline]
pub const fn ptr_size_bits() -> usize {
    mem::size_of::<usize>() * 8
}

/// Must not panic
#[inline]
pub unsafe fn map_in_place<T>(r: &mut T, f: impl FnOnce(T) -> T) {
    ptr::write(r, f(ptr::read(r)));
}
