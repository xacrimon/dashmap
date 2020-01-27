use crossbeam_epoch::Guard;
use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};

pub struct Element<K, V> {
    pub lock: RwLock<()>,
    pub key: K,
    pub hash: u64,
    pub value: UnsafeCell<V>,
}

impl<K, V> Element<K, V> {
    pub fn new(key: K, hash: u64, value: V) -> Self {
        Self {
            lock: RwLock::new(()),
            key,
            hash,
            value: UnsafeCell::new(value),
        }
    }

    pub fn read<'a>(&'a self, guard: Guard) -> ElementReadGuard<'a, K, V> {
        unsafe {
            ElementReadGuard {
                _lock_guard: self.lock.read(),
                _mem_guard: guard,
                key: &self.key,
                value: &*self.value.get(),
            }
        }
    }

    pub fn write<'a>(&'a self, guard: Guard) -> ElementWriteGuard<'a, K, V> {
        unsafe {
            ElementWriteGuard {
                _lock_guard: self.lock.write(),
                _mem_guard: guard,
                key: &self.key,
                value: &mut *self.value.get(),
            }
        }
    }
}

pub struct ElementReadGuard<'a, K, V> {
    _lock_guard: RwLockReadGuard<'a, ()>,
    _mem_guard: Guard,
    key: &'a K,
    value: &'a V,
}

impl<'a, K, V> ElementReadGuard<'a, K, V> {
    pub fn pair(&self) -> (&K, &V) {
        (self.key, self.value)
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }
}

impl<'a, K, V> Deref for ElementReadGuard<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

pub struct ElementWriteGuard<'a, K, V> {
    _lock_guard: RwLockWriteGuard<'a, ()>,
    _mem_guard: Guard,
    key: &'a K,
    value: &'a mut V,
}

impl<'a, K, V> ElementWriteGuard<'a, K, V> {
    pub fn pair(&self) -> (&K, &V) {
        (self.key, self.value)
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        (self.key, self.value)
    }

    pub fn value_mut(&mut self) -> &mut V {
        self.pair_mut().1
    }
}

impl<'a, K, V> Deref for ElementWriteGuard<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a, K, V> DerefMut for ElementWriteGuard<'a, K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}
