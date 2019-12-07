use std::hash::Hash;
use std::ops::{Deref, DerefMut};

pub struct RefInterface<'a, K: Eq + Hash, V> {
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> RefInterface<'a, K, V> {
    #[inline(always)]
    pub(crate) fn new(k: &'a K, v: &'a V) -> Self {
        Self { k, v }
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

impl<'a, K: Eq + Hash, V> Deref for RefInterface<'a, K, V> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMutInterface<'a, K: Eq + Hash, V> {
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> RefMutInterface<'a, K, V> {
    #[inline(always)]
    pub(crate) fn new(k: &'a K, v: &'a mut V) -> Self {
        Self { k, v }
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

impl<'a, K: Eq + Hash, V> Deref for RefMutInterface<'a, K, V> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for RefMutInterface<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}
