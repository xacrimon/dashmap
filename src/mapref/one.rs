use crate::HashMap;
use fxhash::FxBuildHasher;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::{BuildHasher, Hash};
use std::ops::{Deref, DerefMut};

// -- Shared

pub struct Ref<'a, K: Eq + Hash, V, S: BuildHasher = FxBuildHasher> {
    _guard: RwLockReadGuard<'a, HashMap<K, V, S>>,
    k: &'a K,
    v: &'a V,
}

unsafe impl<'a, K: Eq + Hash + Send, V: Send, S: BuildHasher> Send for Ref<'a, K, V, S> {}
unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync, S: BuildHasher> Sync
    for Ref<'a, K, V, S>
{
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> Ref<'a, K, V, S> {
    #[inline(always)]
    pub(crate) fn new(guard: RwLockReadGuard<'a, HashMap<K, V, S>>, k: &'a K, v: &'a V) -> Self {
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

impl<'a, K: Eq + Hash, V, S: BuildHasher> Deref for Ref<'a, K, V, S> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

// --

// -- Unique

pub struct RefMut<'a, K: Eq + Hash, V, S: BuildHasher = FxBuildHasher> {
    _guard: RwLockWriteGuard<'a, HashMap<K, V, S>>,
    k: &'a K,
    v: &'a mut V,
}

unsafe impl<'a, K: Eq + Hash + Send, V: Send, S: BuildHasher> Send for RefMut<'a, K, V, S> {}
unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync, S: BuildHasher> Sync
    for RefMut<'a, K, V, S>
{
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> RefMut<'a, K, V, S> {
    #[inline(always)]
    pub(crate) fn new(
        guard: RwLockWriteGuard<'a, HashMap<K, V, S>>,
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

impl<'a, K: Eq + Hash, V, S: BuildHasher> Deref for RefMut<'a, K, V, S> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> DerefMut for RefMut<'a, K, V, S> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

// --
