use dashmap_shard::HashMap;
use fxhash::FxBuildHasher;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

// -- Shared

pub struct RefMulti<'a, K: Eq + Hash, V> {
    _guard: Arc<RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> RefMulti<'a, K, V> {
    #[inline(always)]
    pub(crate) fn new(
        guard: Arc<RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>>,
        k: &'a K,
        v: &'a V,
    ) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    #[inline(always)]
    pub fn key(&self) -> &K {
        self.k
    }

    #[inline(always)]
    pub fn value(&self) -> &V {
        self.v
    }

    #[inline(always)]
    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }
}

impl<'a, K: Eq + Hash, V> Deref for RefMulti<'a, K, V> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

// --

// -- Unique

pub struct RefMutMulti<'a, K: Eq + Hash, V> {
    _guard: Arc<RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> RefMutMulti<'a, K, V> {
    #[inline(always)]
    pub(crate) fn new(
        guard: Arc<RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>>,
        k: &'a K,
        v: &'a mut V,
    ) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    #[inline(always)]
    pub fn key(&self) -> &K {
        self.k
    }

    #[inline(always)]
    pub fn value(&self) -> &V {
        self.v
    }

    #[inline(always)]
    pub fn value_mut(&mut self) -> &mut V {
        self.v
    }

    #[inline(always)]
    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }

    #[inline(always)]
    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        (self.k, self.v)
    }
}

impl<'a, K: Eq + Hash, V> Deref for RefMutMulti<'a, K, V> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for RefMutMulti<'a, K, V> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

// --
