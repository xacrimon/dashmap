use crate::util::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use crate::alloc::Sarc;
use std::cell::UnsafeCell;
use std::mem::transmute;
use std::ops::{Deref, DerefMut};

pub struct Element<K, V> {
    pub hash: u64,
    pub key: K,
    lock: RwLock<()>,
    value: UnsafeCell<V>,
}

impl<K, V> Element<K, V> {
    pub fn new(key: K, hash: u64, value: V) -> Self {
        Self {
            hash,
            key,
            lock: RwLock::new(()),
            value: UnsafeCell::new(value),
        }
    }

    pub fn read(data: Sarc<Element<K, V>>) -> ElementReadGuard<K, V> {
        unsafe {
            let _lock_guard = transmute(data.lock.read());
            let key = transmute(&data.key);
            let value = transmute(&data.value);
            ElementReadGuard {
                _lock_guard,
                _mem_guard: data,
                key,
                value,
            }
        }
    }

    pub fn write(data: Sarc<Element<K, V>>) -> ElementWriteGuard<K, V> {
        unsafe {
            let _lock_guard = transmute(data.lock.write());
            let key = transmute(&data.key);
            let value = transmute(&data.value);
            ElementWriteGuard {
                _lock_guard,
                _mem_guard: data,
                key,
                value,
            }
        }
    }
}

pub struct ElementReadGuard<K, V> {
    _lock_guard: RwLockReadGuard<'static, ()>,
    _mem_guard: Sarc<Element<K, V>>,
    key: *const K,
    value: *const V,
}

impl<K, V> ElementReadGuard<K, V> {
    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&*self.key, &*self.value) }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }
}

impl<K, V> Deref for ElementReadGuard<K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

pub struct ElementWriteGuard<K, V> {
    _lock_guard: RwLockWriteGuard<'static, ()>,
    _mem_guard: Sarc<Element<K, V>>,
    key: *const K,
    value: *mut V,
}

impl<K, V> ElementWriteGuard<K, V> {
    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&*self.key, &*self.value) }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        unsafe { (&*self.key, &mut *self.value) }
    }

    pub fn value_mut(&mut self) -> &mut V {
        self.pair_mut().1
    }
}

impl<K, V> Deref for ElementWriteGuard<K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<K, V> DerefMut for ElementWriteGuard<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value_mut()
    }
}
