use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};

pub struct Element<K, V> {
    pub lock: RwLock<()>,
    pub key: K,
    pub value: UnsafeCell<V>,
}

impl<K, V> Element<K, V> {
    pub fn new(key: K, value: V) -> Self {
        Self {
            lock: RwLock::new(()),
            key,
            value: UnsafeCell::new(value),
        }
    }

    pub fn read<'a>(&'a self) -> ElementReadGuard<'a, K, V> {
        unsafe {
            ElementReadGuard {
                _guard: self.lock.read(),
                key: &self.key,
                value: &*self.value.get(),
            }
        }
    }

    pub fn write<'a>(&'a self) -> ElementWriteGuard<'a, K, V> {
        unsafe {
            ElementWriteGuard {
                _guard: self.lock.write(),
                key: &self.key,
                value: &mut *self.value.get(),
            }
        }
    }
}

pub struct ElementReadGuard<'a, K, V> {
    pub _guard: RwLockReadGuard<'a, ()>,
    pub key: &'a K,
    pub value: &'a V,
}

impl<'a, K, V> Deref for ElementReadGuard<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub struct ElementWriteGuard<'a, K, V> {
    pub _guard: RwLockWriteGuard<'a, ()>,
    pub key: &'a K,
    pub value: &'a mut V,
}

impl<'a, K, V> Deref for ElementWriteGuard<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, K, V> DerefMut for ElementWriteGuard<'a, K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
