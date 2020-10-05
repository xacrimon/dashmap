use crate::lock::{RwLockReadGuard, RwLockWriteGuard};
use crate::HashMap;
use ahash::RandomState;
use core::hash::{BuildHasher, Hash};
use core::ops::{Deref, DerefMut};
use std::marker::PhantomData;

// -- Shared
pub struct Ref<'a, K, V, S = RandomState> {
    _guard: RwLockReadGuard<'a, ()>,
    k: &'a K,
    v: &'a V,
    p: PhantomData<S>
}

unsafe impl<'a, K: Eq + Hash + Send, V: Send, S: BuildHasher> Send for Ref<'a, K, V, S> {}

unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync, S: BuildHasher> Sync
    for Ref<'a, K, V, S>
{
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> Ref<'a, K, V, S> {
    pub(crate) fn new(guard: RwLockReadGuard<'a, HashMap<K, V, S>>, k: &'a K, v: &'a V) -> Self {
        let guard = unsafe {std::mem::transmute(guard)};
        Self::_new(guard, k, v)
    }
    fn _new(guard: RwLockReadGuard<'a, ()>, k: &'a K, v: &'a V) -> Self {
        Self {
            _guard: guard,
            k,
            v,
            p: Default::default()
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

impl<'a, K: Eq + Hash, V, S: BuildHasher> Deref for Ref<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

// --
// -- Unique
pub struct RefMut<'a, K, V, S = RandomState> {
    guard: RwLockWriteGuard<'a, ()>,
    k: &'a K,
    v: &'a mut V,
    p: PhantomData<S>
}

unsafe impl<'a, K: Eq + Hash + Send, V: Send, S: BuildHasher> Send for RefMut<'a, K, V, S> {}

unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync, S: BuildHasher> Sync
    for RefMut<'a, K, V, S>
{
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> RefMut<'a, K, V, S> {
    pub(crate) fn new(
        guard: RwLockWriteGuard<'a, HashMap<K, V, S>>,
        k: &'a K,
        v: &'a mut V,
    ) -> Self {
        let guard = unsafe { std::mem::transmute(guard) };
        Self { guard, k, v, p: Default::default() }
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

    pub fn downgrade(self) -> Ref<'a, K, V, S> {
        Ref::_new(self.guard.downgrade(), self.k, self.v)
    }
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> Deref for RefMut<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> DerefMut for RefMut<'a, K, V, S> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> Ref<'a, K, V, S> {
    #[inline]
    pub fn map<U, F>(orig: Ref<'a, K, V, S>, f: F) -> Ref<'a, K, U, S>
        where
            F: FnOnce(&V) -> &U,
    {
        let Ref{ _guard, k, v, p } = orig;
        Ref {
            _guard,
            k,
            v: f(v),
            p
        }
    }
}


impl<'a, K: Eq + Hash, V, S: BuildHasher> RefMut<'a, K, V, S> {
    #[inline]
    pub fn map<U, F>(orig: RefMut<'a, K, V, S>, f: F) -> RefMut<'a, K, U, S>
        where
            F: FnOnce(&mut V) -> &mut U,
    {
        let RefMut { guard, k, v, p } = orig;
        RefMut {
            guard,
            k,
            v: f(v),
            p
        }
    }
}

