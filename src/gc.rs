use crate::alloc::ObjectAllocator;
use crate::thread_local::ThreadLocal;
use crate::utils::shim::sync::atomic::{fence, AtomicPtr, AtomicUsize, Ordering};
use std::iter;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr;

fn incmod4(a: &AtomicUsize) -> Option<usize> {
    let current = a.load(Ordering::SeqCst);
    let next = (current + 1) % 4;
    let swapped = a.compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::SeqCst);

    swapped.ok().map(|_| next)
}

fn prev_3(a: usize) -> usize {
    (a + 1) % 4
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
            unsafe {
                ptr::write(node_ptr, data);
            }
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

    fn len(&self) -> usize {
        self.head.load(Ordering::SeqCst)
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
        if self.active.fetch_sub(1, Ordering::SeqCst) == 1 {
            if gc.should_advance() {
                if let Some(can_free) = gc.try_advance() {
                    gc.collect(can_free);
                }
            }
        }
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
            garbage: [
                AtomicPtr::new(new_queue()),
                AtomicPtr::new(new_queue()),
                AtomicPtr::new(new_queue()),
                AtomicPtr::new(new_queue()),
            ],
            _m0: PhantomData,
        }
    }

    fn thread_state(&self) -> &ThreadState {
        self.threads.get(|| ThreadState::new(&self))
    }

    fn collect(&self, epoch: usize) {
        fence(Ordering::SeqCst);
        let new_queue = new_queue();
        let old_queue_ptr = self.garbage[epoch].swap(new_queue, Ordering::SeqCst);
        let old_queue = unsafe { &*old_queue_ptr };

        for tag in old_queue.iter() {
            unsafe {
                self.allocator.deallocate(*tag);
            }
        }

        fence(Ordering::SeqCst);
    }

    fn should_advance(&self) -> bool {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let queue_atomic = unsafe { self.garbage.get_unchecked(epoch) };
        let queue = unsafe { &*queue_atomic.load(Ordering::SeqCst) };
        queue.len() > (QUEUE_CAPACITY / 2)
    }

    fn try_advance(&self) -> Option<usize> {
        fence(Ordering::SeqCst);
        let global_epoch = self.epoch.load(Ordering::SeqCst);

        let can_collect = self
            .threads
            .iter()
            .filter(|state| state.active.load(Ordering::SeqCst) != 0)
            .all(|state| state.epoch.load(Ordering::SeqCst) == global_epoch);

        let ret = if can_collect {
            incmod4(&self.epoch).map(|epoch| prev_3(epoch))
        } else {
            None
        };

        fence(Ordering::SeqCst);
        ret
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
        unsafe {
            (&*queue_ptr).push(tag);
        }
    }
}

impl<T, A: ObjectAllocator<T>> Drop for Gc<T, A> {
    fn drop(&mut self) {
        for queue in &self.garbage {
            let queue = unsafe { Box::from_raw(queue.load(Ordering::SeqCst)) };

            for tag in queue.iter() {
                unsafe {
                    self.allocator.deallocate(*tag);
                }
            }
        }
    }
}
