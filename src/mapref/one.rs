use dashmap_shard::HashMap;
use fxhash::FxBuildHasher;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

// -- Shared

pub struct Ref<'a, K: Eq + Hash, V> {
    _guard: RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> Ref<'a, K, V> {
    pub(crate) fn new(
        guard: RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>,
        k: &'a K,
        v: &'a V,
    ) -> Self {
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

impl<'a, K: Eq + Hash, V> Deref for Ref<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

// --

// -- Unique

pub struct RefMut<'a, K: Eq + Hash, V> {
    _guard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> RefMut<'a, K, V> {
    pub(crate) fn new(
        guard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>,
        k: &'a K,
        v: &'a mut V,
    ) -> Self {
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

impl<'a, K: Eq + Hash, V> Deref for RefMut<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for RefMut<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

// --
