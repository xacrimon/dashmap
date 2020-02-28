use std::mem;

fn low_bits<T>() -> usize {
    (1 << mem::align_of::<T>().trailing_zeros()) - 1
}

fn data_with_tag<T>(data: usize, tag: usize) -> usize {
    (data & !low_bits::<T>()) | (tag & low_bits::<T>())
}

fn decompose_data<T>(data: usize) -> (*mut T, usize) {
    let raw = (data & !low_bits::<T>()) as *mut T;
    let tag = data & low_bits::<T>();
    (raw, tag)
}

pub fn p_tag<T>(p: *const T) -> usize {
    let (_, tag) = decompose_data::<T>(p as usize);
    tag
}

pub fn p_set_tag<T>(p: *const T, tag: usize) -> *mut T {
    data_with_tag::<T>(p as usize, tag) as _
}
