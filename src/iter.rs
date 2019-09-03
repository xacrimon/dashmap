use super::mapref::multiple::{DashMapRefMulti, DashMapRefMutMulti};
use super::DashMap;
use dashmap_shard::HashMap;
use std::sync::Arc;
use super::util;
use fxhash::FxBuildHasher;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::Hash;
use dashmap_shard::hash_map;

type GuardIter<'a, K, V> = (
    Arc<RwLockReadGuard<'a, HashMap<K, V, FxBuildHasher>>>,
    hash_map::Iter<'a, K, V>,
);
type GuardIterMut<'a, K, V> = (
    Arc<RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>>,
    hash_map::IterMut<'a, K, V>,
);

pub struct Iter<'a, K: Eq + Hash, V> {
    map: &'a DashMap<K, V>,
    shard_i: usize,
    current: Option<GuardIter<'a, K, V>>,
}

impl<'a, K: Eq + Hash, V> Iter<'a, K, V> {
    pub(crate) fn new(map: &'a DashMap<K, V>) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V> Iterator for Iter<'a, K, V> {
    type Item = DashMapRefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.as_mut() {
            if let Some(v) = current.1.next() {
                let guard = current.0.clone();
                return Some(DashMapRefMulti::new(guard, v.0, v.1));
            }
        }

        if self.shard_i == self.map.shards().len() - 1 {
            return None;
        }

        let shards = self.map.shards();
        let guard = shards[self.shard_i].read();
        let sref: &HashMap<K, V, FxBuildHasher> = unsafe { util::change_lifetime_const(&*guard) };
        let iter = sref.iter();
        self.current = Some((Arc::new(guard), iter));
        self.shard_i += 1;

        self.next()
    }
}

pub struct IterMut<'a, K: Eq + Hash, V> {
    map: &'a DashMap<K, V>,
    shard_i: usize,
    current: Option<GuardIterMut<'a, K, V>>,
}

impl<'a, K: Eq + Hash, V> IterMut<'a, K, V> {
    pub(crate) fn new(map: &'a DashMap<K, V>) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V> Iterator for IterMut<'a, K, V> {
    type Item = DashMapRefMutMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.as_mut() {
            if let Some(t) = current.1.next() {
                let guard = current.0.clone();
                let k = unsafe { &*(t.0 as *const K) };
                let v = unsafe { &mut *(t.1 as *mut V) };
                return Some(DashMapRefMutMulti::new(guard, k, v));
            }
        }

        if self.shard_i == self.map.shards().len() - 1 {
            return None;
        }

        let shards = self.map.shards();
        let mut guard = shards[self.shard_i].write();
        let sref: &mut HashMap<K, V, FxBuildHasher> = unsafe { util::change_lifetime_mut(&mut *guard) };
        let iter = sref.iter_mut();
        self.current = Some((Arc::new(guard), iter));
        self.shard_i += 1;

        self.next()
    }
}
