use crate::alloc::ObjectAllocator;
use crate::shim::sync::atomic::{AtomicUsize, Ordering};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr;
use thread_local::ThreadLocal;

fn incmod4(a: &AtomicUsize) -> usize {
    loop {
        let current = a.load(Ordering::SeqCst);
        let next = (current + 1) & 3;
        let swapped = a.compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::SeqCst);

        if swapped.is_ok() {
            break next;
        }
    }
}

struct Queue<T> {
    head: AtomicUsize,
    nodes: Box<[MaybeUninit<T>; 255]>,
}

impl<T> Queue<T> {
    fn new() -> Self {
        let nodes = unsafe { MaybeUninit::<[MaybeUninit<T>; 255]>::uninit().assume_init() };

        Self {
            head: AtomicUsize::new(0),
            nodes: Box::new(nodes),
        }
    }

    fn push(&self, node: T) {
        let slot = self.head.fetch_add(1, Ordering::SeqCst);

        unsafe {
            let ptr = self.nodes.get_unchecked(slot).as_ptr() as *mut T;
            ptr::write(ptr, node);
        }
    }

    fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Ordering::SeqCst);

            if head != 0 {
                let slot = head - 1;

                let swapped =
                    self.head
                        .compare_exchange_weak(head, slot, Ordering::SeqCst, Ordering::SeqCst);

                if swapped.is_ok() {
                    let node = unsafe {
                        let ptr = self.nodes.get_unchecked(slot).as_ptr();
                        ptr::read(ptr)
                    };

                    break Some(node);
                }
            } else {
                break None;
            }
        }
    }
}

struct ThreadState {
    active: AtomicUsize,
    epoch: AtomicUsize,
}

impl ThreadState {
    fn new<T, A: ObjectAllocator<T>>(gc: &Gc<T, A>) -> Self {
        let global_epoch = gc.epoch.load(Ordering::SeqCst);

        Self {
            active: AtomicUsize::new(0),
            epoch: AtomicUsize::new(global_epoch),
        }
    }

    fn enter<T, A: ObjectAllocator<T>>(&self, gc: &Gc<T, A>) {
        if self.active.fetch_add(1, Ordering::SeqCst) == 0 {
            let global_epoch = gc.epoch.load(Ordering::SeqCst);
            self.epoch.store(global_epoch, Ordering::SeqCst);
        }
    }

    fn exit<T, A: ObjectAllocator<T>>(&self, gc: &Gc<T, A>) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct Gc<T, A: ObjectAllocator<T>> {
    pub(crate) allocator: A,
    epoch: AtomicUsize,
    threads: ThreadLocal<ThreadState>,
    garbage: [Queue<A::Tag>; 4],
    _m0: PhantomData<T>,
}

impl<T, A: ObjectAllocator<T>> Gc<T, A> {
    pub fn new(allocator: A) -> Self {
        Self {
            allocator,
            epoch: AtomicUsize::new(0),
            threads: ThreadLocal::new(),
            garbage: [
                Queue::new(),
                Queue::new(),
                Queue::new(),
                Queue::new(),
            ],
            _m0: PhantomData,
        }
    }

    fn thread_state(&self) -> &ThreadState {
        self.threads.get_or(|| ThreadState::new(&self))
    }

    pub fn enter(&self) {
        self.thread_state().enter(&self);
    }

    pub fn exit(&self) {
        self.thread_state().exit(&self);
    }

    pub fn retire(&self, tag: A::Tag) {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let queue = unsafe { self.garbage.get_unchecked(epoch) };
        queue.push(tag);
    }
}
