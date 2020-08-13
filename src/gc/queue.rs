use crate::utils::shim::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::cell::UnsafeCell;
use std::cmp;
use std::iter;
use std::mem::MaybeUninit;
use std::ptr;

/// How many elements the queue segment has capacity for.
const QUEUE_CAPACITY: usize = 14;

/// A wait-free append-only queue used for storing garbage.
///
/// Does not call destructors on drop.
pub struct Queue<T> {
    /// The next free slot in the queue.
    head: AtomicUsize,

    /// A pointer to the next queue segment. This is null if there isn't one.
    next: AtomicPtr<Self>,

    /// An array of nodes that may be occupied.
    nodes: [UnsafeCell<MaybeUninit<T>>; QUEUE_CAPACITY],
}

impl<T> Queue<T> {
    /// Create a new queue segment.
    pub fn new() -> *mut Self {
        let nodes = unsafe { MaybeUninit::uninit().assume_init() };

        Box::into_raw(Box::new(Self {
            head: AtomicUsize::new(0),
            next: AtomicPtr::new(ptr::null_mut()),
            nodes,
        }))
    }

    /// Push an item onto the queue.
    pub fn push(&self, data: T) {
        // release is needed so it synchronizes properly with the acquire load in `iter`
        let slot = self.head.fetch_add(1, Ordering::Release);

        if slot >= QUEUE_CAPACITY {
            self.get_next_or_create().push(data);
        } else {
            let node_ptr = self.nodes[slot].get();

            unsafe {
                ptr::write(node_ptr, MaybeUninit::new(data));
            }
        }
    }

    /// Iterate over all elements in this queue segment;
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        // acquire is needed so it synchronizes properly with the increment in `push`
        let top = self.head.load(Ordering::Acquire);
        let mut slot = 0;

        iter::from_fn(move || {
            if slot == top {
                None
            } else {
                let node_ptr = self.nodes[slot].get() as *mut T;
                slot += 1;
                Some(unsafe { &*node_ptr })
            }
        })
    }

    /// How many elements there currently are in the queue segment.
    /// This function loads the length using relaxed ordering and thus it may not be fully accurate
    pub fn len(&self) -> usize {
        cmp::min(self.head.load(Ordering::Relaxed), QUEUE_CAPACITY)
    }

    /// The maxmimum capacity of the queue segment.
    pub fn capacity(&self) -> usize {
        QUEUE_CAPACITY
    }

    /// Get a reference to the next queue segment if it exists.
    pub fn get_next(&self) -> Option<&Self> {
        // acquire is needed here so it synchronizes properly with the
        // rmw in `get_next_or_create`
        //
        // the pointer must always pointer to a valid object or be null
        unsafe { self.next.load(Ordering::Acquire).as_ref() }
    }

    /// Get a reference to the next queue segment, creating it if it doesn't exist
    fn get_next_or_create(&self) -> &Self {
        let mut next = self.next.load(Ordering::Relaxed);

        while next.is_null() {
            let new_queue = Self::new();

            // release is used here so the write becomes visible to the
            // load in `get_next` and the drop implementation
            let did_swap = self.next.compare_exchange_weak(
                next,
                new_queue,
                Ordering::Release,
                Ordering::Relaxed,
            );

            if let Err(actual) = did_swap {
                // drop the allocated queue segment
                // we've previously allocated the segment with a box
                // so it must be valid to drop it with a box
                unsafe {
                    Box::from_raw(new_queue);
                }

                // if the actual value is not null another thread has already created a queue segment for us
                if !actual.is_null() {
                    break;
                }
            } else {
                next = new_queue;
                break;
            }
        }

        // at this point `next` must point to a valid object
        debug_assert!(!next.is_null());
        unsafe { &*next }
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        // acquire needs to so that we synchronize with the rmw in `get_next_or_create`
        // otherwise we may not find all the queue segments that have been allocated
        let next_ptr = self.next.load(Ordering::Acquire);

        if !next_ptr.is_null() {
            // if `next_ptr` is not null it must point to a valid object
            unsafe {
                Box::from_raw(next_ptr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Queue;

    #[test]
    fn push_pop_eq() {
        let queue_ptr = Queue::new();
        let queue = unsafe { &*queue_ptr };
        queue.push(495);
        assert_eq!(queue.iter().count(), 1);
        assert_eq!(queue.iter().next().copied(), Some(495));
    }
}
