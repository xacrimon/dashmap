use crate::util::{
    clear_bit, read_bit, set_bit, u64_read_byte, u64_write_byte, RESIZE_BIT, TOMBSTONE_BIT,
};
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize};

struct Group<T> {
    cache: AtomicU64,
    nodes: [AtomicPtr<T>; 8],
}
