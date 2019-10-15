use std::{mem, ptr};

pub const fn ptr_size_bits() -> usize {
    mem::size_of::<usize>() * 8
}

pub fn map_in_place_2<T, U, F: FnOnce(U, T) -> T>(a: (U, &mut T), f: F) {
    unsafe {
        ptr::write(a.1, f(a.0, ptr::read(a.1)));
    }
}

pub unsafe fn change_lifetime_const<'a, 'b, T>(x: &'a T) -> &'b T {
    &*(x as *const T)
}

pub unsafe fn change_lifetime_mut<'a, 'b, T>(x: &'a mut T) -> &'b mut T {
    &mut *(x as *mut T)
}

#[allow(clippy::cast_ref_to_mut)]
#[allow(clippy::mut_from_ref)]
#[allow(clippy::needless_lifetimes)]
pub unsafe fn to_mut<'a, T>(x: &'a T) -> &'a mut T {
    &mut *(x as *const T as *mut T)
}
