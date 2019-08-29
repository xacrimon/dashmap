use std::mem;

pub const fn ptr_size_bits() -> usize {
    mem::size_of::<usize>() * 8
}
