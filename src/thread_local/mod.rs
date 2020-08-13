mod priority_queue;
mod table;
mod thread_id;

use crate::utils::shim::sync::atomic::{AtomicPtr, Ordering};
use table::Table;

/// A wrapper that keeps different instances of something per thread.
///
/// Think of this as a non global thread-local variable.
/// Threads may occasionally get an old value that another thread previously had.
/// There isn't a nice way to avoid this without compromising on performance.
pub struct ThreadLocal<T: Send + Sync> {
    table: AtomicPtr<Table<T>>,
}

impl<T: Send + Sync> ThreadLocal<T> {
    pub fn new() -> Self {
        let table = Table::new(4, None);
        let table_ptr = Box::into_raw(Box::new(table));

        Self {
            table: AtomicPtr::new(table_ptr),
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
            unsafe { self.insert(id_usize, data) }
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
            match unsafe { table.get_as_owner(key) } {
                Some(x) => Some(unsafe { &*x }),
                None => self.get_slow(key),
            }
        }
    }

    /// Slow path, searches tables recursively.
    fn get_slow(&self, key: usize) -> Option<&T> {
        let mut current = Some(self.table(Ordering::Acquire));

        while let Some(table) = current {
            if key <= table.max_id() {
                if let Some(x) = unsafe { table.get_as_owner(key) } {
                    return Some(unsafe { self.insert(key, x) });
                }
            }

            current = table.previous();
        }

        None
    }

    /// Insert into the top level table.
    ///
    /// # Safety
    /// A key may not be inserted two times with the same top level table.
    unsafe fn insert(&self, key: usize, data: *mut T) -> &T {
        loop {
            let table = self.table(Ordering::Acquire);

            let actual_table = if key > table.max_id() {
                let old_table_ptr = table as *const Table<T> as *mut Table<T>;
                let old_table = Box::from_raw(old_table_ptr);
                let new_table = Table::new(key * 2, Some(old_table));
                let new_table_ptr = Box::into_raw(Box::new(new_table));

                if self
                    .table
                    .compare_exchange_weak(
                        old_table_ptr,
                        new_table_ptr,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    &*new_table_ptr
                } else {
                    continue;
                }
            } else {
                table
            };

            actual_table.set(key, data);
            break &*data;
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
