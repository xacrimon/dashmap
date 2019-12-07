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

unsafe impl<'a, K: Eq + Hash + Send, V: Send> Send for Ref<'a, K, V> {}
unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync> Sync for Ref<'a, K, V> {}

impl<'a, K: Eq + Hash, V> Ref<'a, K, V> {
    #[inline(always)]
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

impl<'a, K: Eq + Hash, V> Deref for Ref<'a, K, V> {
    type Target = V;

    #[inline(always)]
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

unsafe impl<'a, K: Eq + Hash + Send, V: Send> Send for RefMut<'a, K, V> {}
unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync> Sync for RefMut<'a, K, V> {}

impl<'a, K: Eq + Hash, V> RefMut<'a, K, V> {
    #[inline(always)]
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

impl<'a, K: Eq + Hash, V> Deref for RefMut<'a, K, V> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for RefMut<'a, K, V> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

// --
