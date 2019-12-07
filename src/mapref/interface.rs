use crate::interface::{tt_rl_exclusive, tt_rl_shared, BorrowStatus};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

pub struct RefInterface<'a, K: Eq + Hash, V> {
    hash: u64,
    borrows: &'a RefCell<HashMap<u64, BorrowStatus>>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> Drop for RefInterface<'a, K, V> {
    #[inline]
    fn drop(&mut self) {
        tt_rl_shared(self.borrows, self.hash);
    }
}

impl<'a, K: Eq + Hash, V> RefInterface<'a, K, V> {
    #[inline(always)]
    pub(crate) fn new(
        hash: u64,
        borrows: &'a RefCell<HashMap<u64, BorrowStatus>>,
        k: &'a K,
        v: &'a V,
    ) -> Self {
        Self {
            hash,
            borrows,
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

impl<'a, K: Eq + Hash, V> Deref for RefInterface<'a, K, V> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMutInterface<'a, K: Eq + Hash, V> {
    hash: u64,
    borrows: &'a RefCell<HashMap<u64, BorrowStatus>>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> Drop for RefMutInterface<'a, K, V> {
    #[inline]
    fn drop(&mut self) {
        tt_rl_exclusive(self.borrows, self.hash);
    }
}

impl<'a, K: Eq + Hash, V> RefMutInterface<'a, K, V> {
    #[inline(always)]
    pub(crate) fn new(
        hash: u64,
        borrows: &'a RefCell<HashMap<u64, BorrowStatus>>,
        k: &'a K,
        v: &'a mut V,
    ) -> Self {
        Self {
            hash,
            borrows,
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
