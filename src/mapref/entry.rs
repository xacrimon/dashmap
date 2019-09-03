use super::one::DashMapRefMut;
use parking_lot::RwLockWriteGuard;
use dashmap_shard::HashMap;
use fxhash::FxBuildHasher;
use std::hash::Hash;
use std::mem;
use std::ptr;
use crate::util;

pub struct VacantEntry<'a, K: Eq + Hash, V> {
    shard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>,
    key: K,
}

impl<'a, K: Eq + Hash, V> VacantEntry<'a, K, V> {
    pub fn new(shard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>, key: K) -> Self {
        Self { shard, key }
    }

    pub fn insert(mut self, value: V) -> DashMapRefMut<'a, K, V> {
        unsafe {
            let c: K = ptr::read(&mut self.key);
            self.shard.insert(self.key, value);
            let (k, v) = self.shard.get_key_value(&c).unwrap();
            let k = util::change_lifetime_const(k);
            let v = util::change_lifetime_mut(util::to_mut(v));
            let r = DashMapRefMut::new(self.shard, k, v);
            mem::forget(c);
            r
        }
    }

    pub fn into_key(self) -> K {
        self.key
    }

    pub fn key(&self) -> &K {
        &self.key
    }
}

pub struct OccupiedEntry<'a, K: Eq + Hash, V> {
    shard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>,
    elem: (&'a K, &'a mut V),
    key: Option<K>,
}

impl<'a, K: Eq + Hash, V> OccupiedEntry<'a, K, V> {
    pub fn new(shard: RwLockWriteGuard<'a, HashMap<K, V, FxBuildHasher>>, key: Option<K>, elem: (&'a K, &'a mut V)) -> Self {
        Self { shard, elem, key }
    }

    pub fn get(&self) -> &V {
        self.elem.1
    }

    pub fn get_mut(&mut self) -> &mut V {
        self.elem.1
    }

    pub fn insert(&mut self, value: V) -> V {
        mem::replace(self.elem.1, value)
    }

    pub fn into_ref(self) -> DashMapRefMut<'a, K, V> {
        DashMapRefMut::new(self.shard, self.elem.0, self.elem.1)
    }

    pub fn key(&self) -> &K {
        self.elem.0
    }

    pub fn remove(mut self) -> V {
        self.shard.remove(self.elem.0).unwrap()
    }

    pub fn remove_entry(mut self) -> (K, V) {
        self.shard.remove_entry(self.elem.0).unwrap()
    }

    pub fn replace_entry(mut self, value: V) -> (K, V) {
        let nk = self.key.unwrap();
        let p = self.shard.remove_entry(self.elem.0).unwrap();
        self.shard.insert(nk, value);
        p
    }

    pub fn replace_key(self) -> K {
        let r = unsafe { util::to_mut(self.elem.0) };
        mem::replace(r, self.key.unwrap())
    }
}
