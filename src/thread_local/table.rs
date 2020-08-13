use super::thread_id;
use crate::utils::shim::sync::atomic::{AtomicPtr, Ordering};
use std::mem;
use std::ptr;

/// A wait-free table mapping thread ids to pointers.
/// Because we try to keep thread ids low and reuse them
/// this is implemented as a lookup table instead of a hash table.
/// To allow incremental resizing we also store the previous table if any.
pub struct Table<T> {
    buckets: Box<[AtomicPtr<T>]>,
    previous: Option<Box<Self>>,
}

impl<T> Table<T> {
    pub fn new(max: usize, previous: Option<Box<Self>>) -> Self {
        let unsync_buckets = vec![ptr::null_mut::<T>(); max].into_boxed_slice();
        let buckets = unsafe { mem::transmute(unsync_buckets) };

        Self { buckets, previous }
    }

    /// Get the numerically largest thread id this table can store.
    pub fn max_id(&self) -> usize {
        self.buckets.len() - 1
    }

    /// # Safety
    /// - `key` must be below or equal to `self.max_id()`
    /// - `key` must be the id of the calling thread
    pub unsafe fn get_as_owner(&self, key: usize) -> Option<*mut T> {
        debug_assert_eq!(key, thread_id::get() as usize);
        self.get(key, Ordering::Relaxed)
    }

    /// # Safety
    /// - `key` must be below or equal to `self.max_id()`
    unsafe fn get(&self, key: usize, order: Ordering) -> Option<*mut T> {
        debug_assert!(key <= self.max_id());
        let ptr = self.buckets.get_unchecked(key).load(order);

        // empty buckets are represented as null
        if !ptr.is_null() {
            Some(ptr)
        } else {
            None
        }
    }

    /// # Safety
    /// - `key` must be below or equal to `self.max_id()`
    /// - `key` must be the id of the calling thread
    /// - `key` must not have been set earlier
    pub unsafe fn set(&self, key: usize, ptr: *mut T) {
        debug_assert!(key <= self.max_id());
        debug_assert_eq!(key, thread_id::get() as usize);
        let atomic = self.buckets.get_unchecked(key);

        #[cfg(debug_assertions)]
        {
            let old = atomic.compare_and_swap(ptr::null_mut(), ptr, Ordering::Release);
            debug_assert!(old.is_null());
        }

        #[cfg(not(debug_assertions))]
        atomic.store(ptr, Ordering::Release);
    }

    pub fn previous(&self) -> Option<&Self> {
        self.previous.as_deref()
    }

    /// Iterate over all entries in this and its child tables.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter::Local(LocalTableIter {
            table: self,
            position: 0,
            read_set: Vec::new(),
        })
    }
}

impl<T> Drop for Table<T> {
    fn drop(&mut self) {
        for atomic_ptr in &*self.buckets {
            let ptr = atomic_ptr.load(Ordering::Acquire);

            // create a box from the pointer and drop it if it isn't null
            if !ptr.is_null() {
                // if it isn't null `ptr` must be pointing to a valid table
                unsafe {
                    Box::from_raw(ptr);
                }
            }
        }
    }
}

/// An iterator over a table and its child tables.
/// The iterator has 3 different possible states.
/// - `Iter::Local` means it is iterator over the entries in the current table.
/// - `Iter::Chain` means it is iterating over its child tables.
/// - `Iter::Finished` means it has finished iterating over entries
/// in the current table and there was no child table.
pub enum Iter<'a, T> {
    Local(LocalTableIter<'a, T>),
    Chain(Box<Self>),
    Finished,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Local(iter) => {
                if let Some(item) = iter.next() {
                    Some(item)
                } else {
                    if let Some(previous_table) = iter.table.previous() {
                        *self = Self::Chain(Box::new(previous_table.iter()));
                        self.next()
                    } else {
                        *self = Self::Finished;
                        None
                    }
                }
            }

            Self::Chain(child_iter) => child_iter.next(),
            Self::Finished => None,
        }
    }
}

pub struct LocalTableIter<'a, T> {
    table: &'a Table<T>,
    position: usize,
    read_set: Vec<*mut T>,
}

impl<'a, T> Iterator for LocalTableIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.position == self.table.max_id() {
                break None;
            } else {
                let key = self.position;
                self.position += 1;

                if let Some(ptr) = unsafe { self.table.get(key, Ordering::Acquire) } {
                    if ptr.is_null() || self.read_set.contains(&ptr) {
                        continue;
                    } else {
                        self.read_set.push(ptr);
                    }

                    break Some(unsafe { &*ptr });
                }
            }
        }
    }
}
