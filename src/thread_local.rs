use once_cell::sync::Lazy;
use std::cell::UnsafeCell;
use std::collections::BinaryHeap;
use std::hint;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::Mutex;

fn hash(id: u32, bits: u32) -> u32 {
    id.wrapping_mul(0x9E3779B9) >> (32 - bits)
}

struct IdAllocator {
    limit: u32,
    free: BinaryHeap<u32>,
}

impl IdAllocator {
    fn new() -> Self {
        Self {
            limit: u32::MAX,
            free: BinaryHeap::new(),
        }
    }

    fn allocate(&mut self) -> u32 {
        self.free.pop().unwrap_or_else(|| {
            let id = self.limit;
            self.limit -= 1;
            id
        })
    }

    fn deallocate(&mut self, id: u32) {
        self.free.push(id);
    }
}

static THREAD_ID_ALLOCATOR: Lazy<Mutex<IdAllocator>> = Lazy::new(|| Mutex::new(IdAllocator::new()));

struct ThreadId(u32);

impl ThreadId {
    fn new() -> ThreadId {
        ThreadId(THREAD_ID_ALLOCATOR.lock().unwrap().allocate())
    }
}
impl Drop for ThreadId {
    fn drop(&mut self) {
        THREAD_ID_ALLOCATOR.lock().unwrap().deallocate(self.0);
    }
}

thread_local! {
    static THREAD_ID: ThreadId = ThreadId::new();
}

fn thread_id() -> u32 {
    THREAD_ID.with(|x| x.0)
}

pub struct ThreadLocal<T: Send + Sync> {
    table: AtomicPtr<Table<T>>,
    lock: Mutex<usize>,
}

unsafe impl<T: Send + Sync> Sync for ThreadLocal<T> {}

impl<T: Send + Sync> Drop for ThreadLocal<T> {
    fn drop(&mut self) {
        unsafe {
            Box::from_raw(self.table.load(Ordering::SeqCst));
        }
    }
}

struct TableEntry<T: Send + Sync> {
    owner: AtomicU32,
    data: UnsafeCell<Option<Box<T>>>,
}

impl<T: Send + Sync> Clone for TableEntry<T> {
    fn clone(&self) -> TableEntry<T> {
        TableEntry {
            owner: AtomicU32::new(0),
            data: UnsafeCell::new(None),
        }
    }
}

struct Table<T: Send + Sync> {
    buckets: Box<[TableEntry<T>]>,
    hash_bits: u32,
    prev: Option<Box<Self>>,
}

impl<T: Send + Sync> ThreadLocal<T> {
    pub fn new() -> Self {
        let entry = TableEntry {
            owner: AtomicU32::new(0),
            data: UnsafeCell::new(None),
        };
        let table = Table {
            buckets: vec![entry; 2].into_boxed_slice(),
            hash_bits: 1,
            prev: None,
        };
        ThreadLocal {
            table: AtomicPtr::new(Box::into_raw(Box::new(table))),
            lock: Mutex::new(0),
        }
    }

    pub fn get_or<F>(&self, create: F) -> &T
    where
        F: FnOnce() -> T,
    {
        let id = thread_id();

        match self.get_fast(id) {
            Some(x) => x,
            None => self.insert(id, Box::new(create()), true),
        }
    }

    fn lookup(id: u32, table: &Table<T>) -> Option<&UnsafeCell<Option<Box<T>>>> {
        for entry in table
            .buckets
            .iter()
            .cycle()
            .skip(hash(id, table.hash_bits) as usize)
        {
            let owner = entry.owner.load(Ordering::SeqCst);
            if owner == id {
                return Some(&entry.data);
            }
            if owner == 0 {
                return None;
            }
        }
        unsafe {
            hint::unreachable_unchecked();
        }
    }

    fn raw_iter(&self) -> RawIter<T> {
        RawIter {
            remaining: *self.lock.lock().unwrap(),
            index: 0,
            table: self.table.load(Ordering::SeqCst),
        }
    }

    pub fn iter<'a>(&'a self) -> Iter<'a, T> {
        Iter {
            raw: self.raw_iter(),
            marker: PhantomData,
        }
    }

    fn get_fast(&self, id: u32) -> Option<&T> {
        let table = unsafe { &*self.table.load(Ordering::Acquire) };
        match Self::lookup(id, table) {
            Some(x) => unsafe { Some((*x.get()).as_ref().unwrap()) },
            None => self.get_slow(id, table),
        }
    }

    fn get_slow(&self, id: u32, table_top: &Table<T>) -> Option<&T> {
        let mut current = &table_top.prev;
        while let Some(ref table) = *current {
            if let Some(x) = Self::lookup(id, table) {
                let data = unsafe { (*x.get()).take().unwrap() };
                return Some(self.insert(id, data, false));
            }
            current = &table.prev;
        }
        None
    }

    fn insert(&self, id: u32, data: Box<T>, new: bool) -> &T {
        let mut count = self.lock.lock().unwrap();
        if new {
            *count += 1;
        }
        let table_raw = self.table.load(Ordering::Relaxed);
        let table = unsafe { &*table_raw };

        let table = if *count > table.buckets.len() * 3 / 4 {
            let entry = TableEntry {
                owner: AtomicU32::new(0),
                data: UnsafeCell::new(None),
            };
            let new_table = Box::into_raw(Box::new(Table {
                buckets: vec![entry; table.buckets.len() * 2].into_boxed_slice(),
                hash_bits: table.hash_bits + 1,
                prev: unsafe { Some(Box::from_raw(table_raw)) },
            }));
            self.table.store(new_table, Ordering::Release);
            unsafe { &*new_table }
        } else {
            table
        };

        for entry in table
            .buckets
            .iter()
            .cycle()
            .skip(hash(id, table.hash_bits) as usize)
        {
            let owner = entry.owner.load(Ordering::Relaxed);
            if owner == 0 {
                unsafe {
                    entry.owner.store(id, Ordering::Relaxed);
                    *entry.data.get() = Some(data);
                    return (*entry.data.get()).as_ref().unwrap();
                }
            }
            if owner == id {
                unsafe {
                    return (*entry.data.get()).as_ref().unwrap();
                }
            }
        }
        unsafe {
            hint::unreachable_unchecked();
        }
    }
}

struct RawIter<T: Send + Sync> {
    remaining: usize,
    index: usize,
    table: *const Table<T>,
}

impl<T: Send + Sync> Iterator for RawIter<T> {
    type Item = *mut Option<Box<T>>;

    fn next(&mut self) -> Option<*mut Option<Box<T>>> {
        if self.remaining == 0 {
            return None;
        }

        loop {
            let entries = unsafe { &(*self.table).buckets[..] };
            while self.index < entries.len() {
                let val = entries[self.index].data.get();
                self.index += 1;
                if unsafe { (*val).is_some() } {
                    self.remaining -= 1;
                    return Some(val);
                }
            }
            self.index = 0;
            self.table = unsafe { &**(*self.table).prev.as_ref().unwrap() };
        }
    }
}

pub struct Iter<'a, T: Send + Sync + 'a> {
    raw: RawIter<T>,
    marker: PhantomData<&'a ThreadLocal<T>>,
}

impl<'a, T: Send + Sync + 'a> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.raw
            .next()
            .map(|x| unsafe { &**(*x).as_ref().unwrap() })
    }
}
