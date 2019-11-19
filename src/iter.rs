use super::mapref::multiple::{RefMulti, RefMutMulti};
use super::util;
use crate::t::Map;
use dashmap_shard::hash_map;
use dashmap_shard::HashMap;
use fxhash::FxBuildHasher;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::Hash;
use std::sync::Arc;

type GuardIter<'a, K, V> = (
    Arc<RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>>,
    hash_map::Iter<'a, K, V>,
);
type GuardIterMut<'a, K, V> = (
    Arc<RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>>,
    hash_map::IterMut<'a, K, V>,
);

pub struct Iter<'a, K: Eq + Hash, V, M: Map<'a, K, V>> {
    map: &'a M,
    shard_i: usize,
    current: Option<GuardIter<'a, K, V>>,
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> Iter<'a, K, V, M> {
    pub(crate) fn new(map: &'a M) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> Iterator for Iter<'a, K, V, M> {
    type Item = RefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.as_mut() {
            if let Some(v) = current.1.next() {
                let guard = current.0.clone();
                return Some(RefMulti::new(guard, v.0, v.1));
            }
        }

        if self.shard_i == self.map._shard_count() - 1 {
            return None;
        }

        let guard = self.map._yield_read_shard(self.shard_i);
        let sref: &HashMap<K, V, FxBuildHasher> = unsafe { util::change_lifetime_const(&*guard) };
        let iter = sref.iter();
        self.current = Some((Arc::new(guard), iter));
        self.shard_i += 1;

        self.next()
    }
}

pub struct IterMut<'a, K: Eq + Hash, V, M: Map<'a, K, V>> {
    map: &'a M,
    shard_i: usize,
    current: Option<GuardIterMut<'a, K, V>>,
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> IterMut<'a, K, V, M> {
    pub(crate) fn new(map: &'a M) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> Iterator for IterMut<'a, K, V, M> {
    type Item = RefMutMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.as_mut() {
            if let Some(t) = current.1.next() {
                let guard = current.0.clone();
                let k = unsafe { &*(t.0 as *const K) };
                let v = unsafe { &mut *(t.1 as *mut V) };
                return Some(RefMutMulti::new(guard, k, v));
            }
        }

        if self.shard_i == self.map._shard_count() - 1 {
            return None;
        }

        let mut guard = unsafe { self.map._yield_write_shard(self.shard_i) };
        let sref: &mut HashMap<K, V, FxBuildHasher> =
            unsafe { util::change_lifetime_mut(&mut *guard) };
        let iter = sref.iter_mut();
        self.current = Some((Arc::new(guard), iter));
        self.shard_i += 1;

        self.next()
    }
}
