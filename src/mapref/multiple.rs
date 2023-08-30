use crate::lock::{RwLockReadGuard, RwLockWriteGuard};
use crate::HashMap;
use core::ops::{Deref, DerefMut};
use std::collections::hash_map::RandomState;
use std::sync::Arc;

pub struct RefMulti<'a, K, V, S = RandomState> {
    _guard: Arc<RwLockReadGuard<'a, HashMap<K, V, S>>>,
    k: *const K,
    v: *const V,
}

unsafe impl<'a, K: Sync, V: Sync, S> Send for RefMulti<'a, K, V, S> {}
unsafe impl<'a, K: Sync, V: Sync, S> Sync for RefMulti<'a, K, V, S> {}

impl<'a, K, V, S> RefMulti<'a, K, V, S> {
    pub(crate) unsafe fn new(
        guard: Arc<RwLockReadGuard<'a, HashMap<K, V, S>>>,
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

impl<'a, K, V, S> Deref for RefMulti<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMutMulti<'a, K, V, S = RandomState> {
    _guard: Arc<RwLockWriteGuard<'a, HashMap<K, V, S>>>,
    k: *const K,
    v: *mut V,
}

unsafe impl<'a, K: Sync, V: Sync, S> Send for RefMutMulti<'a, K, V, S> {}
unsafe impl<'a, K: Sync, V: Sync, S> Sync for RefMutMulti<'a, K, V, S> {}

impl<'a, K, V, S> RefMutMulti<'a, K, V, S> {
    pub(crate) unsafe fn new(
        guard: Arc<RwLockWriteGuard<'a, HashMap<K, V, S>>>,
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

impl<'a, K, V, S> Deref for RefMutMulti<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K, V, S> DerefMut for RefMutMulti<'a, K, V, S> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}
