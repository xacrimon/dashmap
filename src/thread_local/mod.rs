mod priority_queue;
mod table;
mod thread_id;

use crate::utils::shim::sync::{
    atomic::{AtomicPtr, Ordering},
    Mutex,
};
use table::Table;

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

    pub fn get<F>(&self, create: F) -> &T
    where
        F: FnOnce() -> T,
    {
        let id = thread_id::get() as usize;

        self.get_fast(id).unwrap_or_else(|| {
            let data = Box::into_raw(Box::new(create()));
            self.insert(id, data, true)
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.table().iter()
    }

    fn table(&self) -> &Table<T> {
        unsafe { &*self.table.load(Ordering::SeqCst) }
    }

    fn get_fast(&self, key: usize) -> Option<&T> {
        let table = self.table();

        if key > table.max_id() {
            None
        } else {
            match table.get(key) {
                Some(x) => Some(unsafe { &*x }),
                None => self.get_slow(key, table),
            }
        }
    }

    fn get_slow(&self, key: usize, table_top: &Table<T>) -> Option<&T> {
        let mut current = table_top.previous();

        while let Some(table) = current {
            if let Some(x) = table.get(key) {
                return Some(self.insert(key, x, false));
            }
            current = table.previous();
        }

        None
    }

    fn insert(&self, key: usize, data: *mut T, new: bool) -> &T {
        let mut count = self.lock.lock().unwrap();

        if new {
            *count += 1;
        }

        let table = self.table();
        let table_ptr = table as *const Table<T> as *mut Table<T>;

        let table = if key > table.max_id() {
            let old_table = unsafe { Box::from_raw(table_ptr) };
            let new_table = Table::new(key * 2, Some(old_table));
            let new_table_ptr = Box::into_raw(Box::new(new_table));
            self.table.store(new_table_ptr, Ordering::SeqCst);
            unsafe { &*new_table_ptr }
        } else {
            table
        };

        table.set(key, data);
        unsafe { &*data }
    }
}

impl<T: Send + Sync> Drop for ThreadLocal<T> {
    fn drop(&mut self) {
        let table_ptr = self.table.load(Ordering::SeqCst);

        unsafe {
            Box::from_raw(table_ptr);
        }
    }
}
