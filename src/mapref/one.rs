use dashmap_shard::HashMap;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use fxhash::FxBuildHasher;

// -- Shared

pub struct DashMapRef<'a, K: Eq + Hash, V> {
    _guard: RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> DashMapRef<'a, K, V> {
    pub(crate) fn new(guard: RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>, k: &'a K, v: &'a V) -> Self {
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

impl<'a, K: Eq + Hash, V> Deref for DashMapRef<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

// --

// -- Unique

pub struct DashMapRefMut<'a, K: Eq + Hash, V> {
    _guard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> DashMapRefMut<'a, K, V> {
    pub(crate) fn new(guard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>, k: &'a K, v: &'a mut V) -> Self {
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

impl<'a, K: Eq + Hash, V> Deref for DashMapRefMut<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for DashMapRefMut<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

// --
