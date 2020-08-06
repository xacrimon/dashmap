//! A fast concurrent memory reclaimer.
//! This is based on the Hyaline algorithm from https://arxiv.org/abs/1905.07903.

use crate::alloc::ObjectAllocator;
use crate::shim::sync::atomic::{AtomicUsize, Ordering};
use std::marker::PhantomData;
use std::mem::MaybeUninit;

struct Queue {
    head: AtomicUsize,
    nodes: [MaybeUninit<usize>; 255],
}

impl Queue {
    fn new() -> Queue {
        Self {
            head: AtomicUsize::new(0),
            nodes: [MaybeUninit::uninit(); 255],
        }
    }

    fn push(&self, node: usize) {
        let slot = self.head.fetch_add(1, Ordering::SeqCst) - 1;

        unsafe {
            let ptr = self.nodes.get_unchecked(slot).as_ptr() as *mut usize;
            *ptr = node;
        }
    }

    fn pop(&self) -> Option<usize> {
        loop {
            let head = self.head.load(Ordering::SeqCst);

            if head != 0 {
                let slot = head - 1;

                let swapped =
                    self.head
                        .compare_exchange_weak(head, slot, Ordering::SeqCst, Ordering::SeqCst);

                if swapped.is_ok() {
                    let node = unsafe { *self.nodes.get_unchecked(slot).as_ptr() };
                    break Some(node);
                }
            } else {
                break None;
            }
        }
    }
}

pub struct Gc<T, A> {
    pub(crate) allocator: A,
    href: AtomicUsize,
    garbage: Queue,
    _m0: PhantomData<T>,
}

impl<T, A: ObjectAllocator<T>> Gc<T, A> {
    pub fn new(allocator: A) -> Self {
        Self {
            allocator,
            href: AtomicUsize::new(0),
            garbage: Queue::new(),
            _m0: PhantomData,
        }
    }
}
