use crate::utils::shim::sync::atomic::{AtomicPtr, Ordering};
use std::mem;
use std::ptr;

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

    pub fn max_id(&self) -> usize {
        self.buckets.len() - 1
    }

    pub fn get(&self, key: usize) -> Option<*mut T> {
        let ptr = self.buckets[key].load(Ordering::SeqCst);

        if !ptr.is_null() {
            Some(ptr)
        } else {
            None
        }
    }

    pub fn set(&self, key: usize, ptr: *mut T) {
        self.buckets[key].store(ptr, Ordering::SeqCst);
    }

    pub fn previous(&self) -> Option<&Self> {
        self.previous.as_deref()
    }

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
            let ptr = atomic_ptr.load(Ordering::SeqCst);

            if !ptr.is_null() {
                unsafe {
                    Box::from_raw(ptr);
                }
            }
        }
    }
}

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

                if let Some(ptr) = self.table.get(key) {
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
