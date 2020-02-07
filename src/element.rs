use crate::util::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::cell::UnsafeCell;
use std::mem::transmute;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct ElementInner<K, V> {
    pub key: K,
    lock: RwLock<()>,
    value: UnsafeCell<V>,
}

pub struct Element<K, V> {
    pub hash: u64,
    pub inner: Arc<ElementInner<K, V>>,
}

impl<K, V> Element<K, V> {
    pub fn new(key: K, hash: u64, value: V) -> Self {
        let inner = Arc::new(ElementInner {
            key,
            lock: RwLock::new(()),
            value: UnsafeCell::new(value),
        });

        Self { hash, inner }
    }

    pub fn read(&self) -> ElementReadGuard<K, V> {
        let _lock_guard = unsafe { transmute(self.inner.lock.read()) };
        let data = self.inner.clone();

        ElementReadGuard { _lock_guard, data }
    }

    pub fn write(&self) -> ElementWriteGuard<K, V> {
        let _lock_guard = unsafe { transmute(self.inner.lock.write()) };
        let data = self.inner.clone();

        ElementWriteGuard { _lock_guard, data }
    }
}

pub struct ElementReadGuard<K, V> {
    _lock_guard: RwLockReadGuard<'static, ()>,
    data: Arc<ElementInner<K, V>>,
}

impl<K, V> ElementReadGuard<K, V> {
    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&self.data.key, &*self.data.value.get()) }
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
    data: Arc<ElementInner<K, V>>,
}

impl<K, V> ElementWriteGuard<K, V> {
    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&self.data.key, &*self.data.value.get()) }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        unsafe { (&self.data.key, &mut *self.data.value.get()) }
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
