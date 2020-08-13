mod epoch;
mod queue;
mod thread_state;

use crate::alloc::ObjectAllocator;
use crate::thread_local::ThreadLocal;
use crate::utils::shim::sync::{
    atomic::{AtomicPtr, Ordering},
    Arc,
};
use epoch::{AtomicEpoch, Epoch};
use queue::Queue;
use std::ptr;
use thread_state::{EbrState, ThreadState};

pub struct Gc<T, A>
where
    A: ObjectAllocator<T>,
{
    allocator: Arc<A>,
    epoch: AtomicEpoch,
    threads: ThreadLocal<ThreadState<Self>>,
    map_garbage: [AtomicPtr<Queue<A::Tag>>; 4],
}

impl<T, A> Gc<T, A>
where
    A: ObjectAllocator<T>,
{
    pub fn new(allocator: Arc<A>) -> Self {
        Self {
            allocator,
            epoch: AtomicEpoch::new(Epoch::Zero),
            threads: ThreadLocal::new(),
            map_garbage: [
                AtomicPtr::new(Queue::new()),
                AtomicPtr::new(Queue::new()),
                AtomicPtr::new(Queue::new()),
                AtomicPtr::new(Queue::new()),
            ],
        }
    }

    pub fn allocator(&self) -> &Arc<A> {
        &self.allocator
    }

    fn thread_state(&self) -> &ThreadState<Self> {
        self.threads.get(|id| ThreadState::new(&self, id))
    }

    fn get_map_garbage(&self, epoch: Epoch) -> &Queue<A::Tag> {
        let raw_epoch: usize = epoch.into();
        let atomic_queue = &self.map_garbage[raw_epoch];
        unsafe { &*atomic_queue.load(Ordering::SeqCst) }
    }

    pub fn enter(&self) {
        self.thread_state().enter(&self);
    }

    pub fn exit(&self) {
        self.thread_state().exit(&self);
    }

    pub fn is_active(&self) -> bool {
        self.thread_state().is_active()
    }

    pub fn retire(&self, tag: A::Tag) {
        let epoch = self.epoch.load();
        let queue = self.get_map_garbage(epoch);
        queue.push(tag);
    }

    fn try_advance(&self) -> Result<Epoch, ()> {
        let global_epoch = self.epoch.load();

        let can_collect = self
            .threads
            .iter()
            .filter(|state| state.is_active())
            .all(|state| state.load_epoch() == global_epoch);

        if can_collect {
            self.epoch.try_advance(global_epoch)
        } else {
            Err(())
        }
    }

    unsafe fn collect(&self, epoch: Epoch, replace: bool) {
        let raw_epoch: usize = epoch.into();

        let new_queue_ptr = if replace {
            Queue::new()
        } else {
            ptr::null_mut()
        };

        let old_queue_ptr = self.map_garbage[raw_epoch].swap(new_queue_ptr, Ordering::SeqCst);
        let mut maybe_queue = Some(&*old_queue_ptr);

        while let Some(queue) = maybe_queue {
            for tag in queue.iter() {
                self.allocator.deallocate(*tag);
            }

            maybe_queue = queue.get_next();
        }
    }
}

impl<T, A> EbrState for Gc<T, A>
where
    A: ObjectAllocator<T>,
{
    type T = T;
    type A = A;

    fn load_epoch(&self) -> Epoch {
        self.epoch.load()
    }

    fn should_advance(&self) -> bool {
        let epoch = self.epoch.load();
        let queue = self.get_map_garbage(epoch);
        queue.len() >= (queue.capacity() / 2)
    }

    fn try_cycle(&self) {
        if let Ok(epoch) = self.try_advance() {
            let safe_epoch = epoch.next();

            unsafe {
                self.collect(safe_epoch, true);
            }
        }
    }
}

impl<T, A> Drop for Gc<T, A>
where
    A: ObjectAllocator<T>,
{
    fn drop(&mut self) {
        unsafe {
            self.collect(Epoch::Zero, false);
            self.collect(Epoch::One, false);
            self.collect(Epoch::Two, false);
            self.collect(Epoch::Three, false);
        }
    }
}
