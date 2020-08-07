use crate::alloc::ObjectAllocator;
use crate::shim::sync::atomic::{AtomicUsize, Ordering, AtomicPtr};
use std::marker::PhantomData;
use std::ptr;
use thread_local::ThreadLocal;
use std::mem::MaybeUninit;
use std::iter;

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

const QUEUE_CAPACITY: usize = 255;

struct Queue<T> {
    head: AtomicUsize,
    nodes: [MaybeUninit<T>; QUEUE_CAPACITY],
}

impl<T> Queue<T> {
    fn new() -> Self {
        let nodes = unsafe { MaybeUninit::uninit().assume_init() };

        Self {
            head: AtomicUsize::new(0),
            nodes,
        }
    }

    fn push(&self, data: T) -> bool {
        let slot = self.head.fetch_add(1, Ordering::SeqCst);

        if slot >= QUEUE_CAPACITY {
            false
        } else {
            let node_ptr = self.nodes[slot].as_ptr() as *mut T;
            unsafe { ptr::write(node_ptr, data); }
            true
        }
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        let top = self.head.load(Ordering::SeqCst);
        let mut slot = 0;

        iter::from_fn(move || {
            if slot == top {
                None
            } else {
                let node_ptr = self.nodes[slot].as_ptr();
                slot += 1;
                Some(unsafe { &*node_ptr })
            }
        })
    }

    fn is_empty(&self) -> bool {
        self.head.load(Ordering::SeqCst) == 0
    }
}

fn new_queue<T>() -> *mut Queue<T> {
    Box::into_raw(Box::new(Queue::new()))
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
    garbage: [AtomicPtr<Queue<A::Tag>>; 4],
    _m0: PhantomData<T>,
}

impl<T, A: ObjectAllocator<T>> Gc<T, A> {
    pub fn new(allocator: A) -> Self {
        Self {
            allocator,
            epoch: AtomicUsize::new(0),
            threads: ThreadLocal::new(),
            garbage: [AtomicPtr::new(new_queue()), AtomicPtr::new(new_queue()), AtomicPtr::new(new_queue()), AtomicPtr::new(new_queue())],
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
        let queue_ptr = unsafe { self.garbage.get_unchecked(epoch).load(Ordering::SeqCst) };
        unsafe { (&*queue_ptr).push(tag); }
    }
}

impl<T, A: ObjectAllocator<T>> Drop for Gc<T, A> {
    fn drop(&mut self) {
        for queue in &self.garbage {
            let queue = unsafe { Box::from_raw(queue.load(Ordering::SeqCst)) };

            for tag in queue.iter()  {
                unsafe {
                    self.allocator.deallocate(*tag);
                }
            }
        }
    }
}
