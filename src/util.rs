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

pub unsafe fn to_mut<'a, T>(x: &'a T) -> &'a mut T {
    &mut *(x as *const T as *mut T)
}

pub fn swap_nonoverlapping<T>(x: *mut T, y: *mut T) {
    let q = mem::size_of::<T>();
    assert!(x as usize + q < y as usize || x as usize > y as usize + q);
    let x = x as *mut u8;
    let y = y as *mut u8;
    unsafe {
        for i in 0..q {
            *((x as usize + i) as *mut u8) ^= *((y as usize + i) as *mut u8);
            *((y as usize + i) as *mut u8) ^= *((x as usize + i) as *mut u8);
            *((x as usize + i) as *mut u8) ^= *((y as usize + i) as *mut u8);
        }
    }
}
