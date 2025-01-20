use crate::lock::{RwLockReadGuard, RwLockWriteGuard};
use crate::HashMap;
use core::hash::Hash;
use core::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct RefMulti<'a, K, V> {
    _guard: Arc<RwLockReadGuard<'a, HashMap<K, V>>>,
    k: *const K,
    v: *const V,
}

unsafe impl<K: Eq + Hash + Sync, V: Sync> Send for RefMulti<'_, K, V> {}
unsafe impl<K: Eq + Hash + Sync, V: Sync> Sync for RefMulti<'_, K, V> {}

impl<'a, K: Eq + Hash, V> RefMulti<'a, K, V> {
    pub(crate) unsafe fn new(
        guard: Arc<RwLockReadGuard<'a, HashMap<K, V>>>,
        k: *const K,
        v: *const V,
    ) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&*self.k, &*self.v) }
    }
}

impl<K: Eq + Hash, V> Deref for RefMulti<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMutMulti<'a, K, V> {
    _guard: Arc<RwLockWriteGuard<'a, HashMap<K, V>>>,
    k: *const K,
    v: *mut V,
}

unsafe impl<K: Eq + Hash + Sync, V: Sync> Send for RefMutMulti<'_, K, V> {}
unsafe impl<K: Eq + Hash + Sync, V: Sync> Sync for RefMutMulti<'_, K, V> {}

impl<'a, K: Eq + Hash, V> RefMutMulti<'a, K, V> {
    pub(crate) unsafe fn new(
        guard: Arc<RwLockWriteGuard<'a, HashMap<K, V>>>,
        k: *const K,
        v: *mut V,
    ) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn value_mut(&mut self) -> &mut V {
        self.pair_mut().1
    }

    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&*self.k, &*self.v) }
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        unsafe { (&*self.k, &mut *self.v) }
    }
}

impl<K: Eq + Hash, V> Deref for RefMutMulti<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<K: Eq + Hash, V> DerefMut for RefMutMulti<'_, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}
