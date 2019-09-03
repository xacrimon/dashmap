use hashbrown::HashMap;
use std::sync::Arc;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

// -- Shared

pub struct DashMapRefMulti<'a, K: Eq + Hash, V> {
    _guard: Arc<RwLockReadGuard<'a, HashMap<K, V>>>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> DashMapRefMulti<'a, K, V> {
    pub(crate) fn new(guard: Arc<RwLockReadGuard<'a, HashMap<K, V>>>, k: &'a K, v: &'a V) -> Self {
        Self { _guard: guard, k, v }
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

impl<'a, K: Eq + Hash, V> Deref for DashMapRefMulti<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

// --

// -- Unique

pub struct DashMapRefMutMulti<'a, K: Eq + Hash, V> {
    _guard: Arc<RwLockWriteGuard<'a, HashMap<K, V>>>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> DashMapRefMutMulti<'a, K, V> {
    pub(crate) fn new(guard: Arc<RwLockWriteGuard<'a, HashMap<K, V>>>, k: &'a K, v: &'a mut V) -> Self {
        Self { _guard: guard, k, v }
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

impl<'a, K: Eq + Hash, V> Deref for DashMapRefMutMulti<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for DashMapRefMutMulti<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

// --
