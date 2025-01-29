use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use core::hash::Hash;
use core::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct RefMulti<'a, K, V> {
    _guard: Arc<RwLockReadGuardDetached<'a>>,
    k: &'a K,
    v: &'a V,
}

impl<K, V> Clone for RefMulti<'_, K, V> {
    fn clone(&self) -> Self {
        Self {
            _guard: self._guard.clone(),
            k: self.k,
            v: self.v,
        }
    }
}

impl<'a, K: Eq + Hash, V> RefMulti<'a, K, V> {
    pub(crate) fn new(guard: Arc<RwLockReadGuardDetached<'a>>, k: &'a K, v: &'a V) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &K {
        self.k
    }

    pub fn value(&self) -> &V {
        self.v
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }
}

impl<K: Eq + Hash, V> Deref for RefMulti<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMutMulti<'a, K, V> {
    _guard: Arc<RwLockWriteGuardDetached<'a>>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> RefMutMulti<'a, K, V> {
    pub(crate) fn new(guard: Arc<RwLockWriteGuardDetached<'a>>, k: &'a K, v: &'a mut V) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &K {
        self.k
    }

    pub fn value(&self) -> &V {
        self.v
    }

    pub fn value_mut(&mut self) -> &mut V {
        self.v
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        (self.k, self.v)
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
