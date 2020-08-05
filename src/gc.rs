//! A fast concurrent memory reclaimer.
//! This is based on the Hyaline-1 algorithm from https://arxiv.org/abs/1905.07903.

use crate::alloc::ObjectAllocator;
use std::marker::PhantomData;
use crate::shim::sync::Arc;

pub struct Gc<T, A> {
    pub(crate) allocator: Arc<A>,
    _m0: PhantomData<T>,
}

impl<T, A: ObjectAllocator<T>> Gc<T, A> {
    pub fn new(allocator: Arc<A>) -> Self {
        Self {
            allocator,
            _m0: PhantomData,
        }
    }
}
