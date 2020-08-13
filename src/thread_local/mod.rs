mod priority_queue;
mod table;
mod thread_id;

use crate::utils::{
    hint::UnwrapUnchecked,
    shim::sync::{
        atomic::{AtomicPtr, Ordering},
        Mutex,
    },
};
use table::Table;

/// A wrapper that keeps different instances of something per thread.
///
/// Think of this as a non global thread-local variable.
/// Threads may occasionally get an old value that another thread previously had.
/// There isn't a nice way to avoid this without compromising on performance.
pub struct ThreadLocal<T: Send + Sync> {
    table: AtomicPtr<Table<T>>,
    lock: Mutex<usize>,
}

impl<T: Send + Sync> ThreadLocal<T> {
    pub fn new() -> Self {
        let table = Table::new(4, None);
        let table_ptr = Box::into_raw(Box::new(table));

        Self {
            table: AtomicPtr::new(table_ptr),
            lock: Mutex::new(0),
        }
    }

    /// Get the value for this thread or initialize it with the given function if it doesn't exist.
    pub fn get<F>(&self, create: F) -> &T
    where
        F: FnOnce(u32) -> T,
    {
        let id = thread_id::get();
        let id_usize = id as usize;

        self.get_fast(id_usize).unwrap_or_else(|| {
            let data = Box::into_raw(Box::new(create(id)));
            self.insert(id_usize, data, true)
        })
    }

    /// Iterate over values.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.table(Ordering::Acquire).iter()
    }

    fn table(&self, order: Ordering) -> &Table<T> {
        unsafe { &*self.table.load(order) }
    }

    // Fast path, checks the top level table.
    fn get_fast(&self, key: usize) -> Option<&T> {
        let table = self.table(Ordering::Relaxed);

        if key > table.max_id() {
            None
        } else {
            match unsafe { table.get(key) } {
                Some(x) => Some(unsafe { &*x }),
                None => self.get_slow(key, table),
            }
        }
    }

    /// Slow path, searches tables recursively.
    fn get_slow(&self, key: usize, table_top: &Table<T>) -> Option<&T> {
        let mut current = table_top.previous();

        while let Some(table) = current {
            if key <= table.max_id() {
                if let Some(x) = unsafe { table.get(key) } {
                    return Some(self.insert(key, x, false));
                }
            }

            current = table.previous();
        }

        None
    }

    /// Insert into the top level table.
    fn insert(&self, key: usize, data: *mut T, new: bool) -> &T {
        let mut count = unsafe { self.lock.lock().unwrap_unchecked() };

        if new {
            *count += 1;
        }

        let table = self.table(Ordering::Relaxed);
        let table_ptr = table as *const Table<T> as *mut Table<T>;

        let table = if key > table.max_id() {
            let old_table = unsafe { Box::from_raw(table_ptr) };
            let new_table = Table::new(key * 2, Some(old_table));
            let new_table_ptr = Box::into_raw(Box::new(new_table));
            self.table.store(new_table_ptr, Ordering::Release);
            unsafe { &*new_table_ptr }
        } else {
            table
        };

        unsafe {
            table.set(key, data);
            &*data
        }
    }
}

impl<T: Send + Sync> Drop for ThreadLocal<T> {
    fn drop(&mut self) {
        let table_ptr = self.table.load(Ordering::Acquire);

        // the table must always be valid, this drops it and its child tables.
        unsafe {
            Box::from_raw(table_ptr);
        }
    }
}
