use super::DashMap;
use super::mapref::multiple::{DashMapRefMulti, DashMapRefMutMulti};
use std::sync::Arc;
use hashbrown::HashMap;
use owning_ref::{OwningRef, OwningRefMut};
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use std::cell::UnsafeCell;
use hashbrown::hash_map;

type GuardIter<'a, K, V> = (Arc<RwLockReadGuard<'a, HashMap<K, V>>>, hash_map::Iter<'a, K, V>);

pub struct Iter<'a, K: Eq + Hash, V> {
    map: &'a DashMap<K, V>,
    shard_i: usize,
    current: Option<GuardIter<'a, K, V>>,
}

impl<'a, K: Eq + Hash, V> Iterator for Iter<'a, K, V> {
    type Item = DashMapRefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.as_mut() {
            if let Some(v) = current.1.next() {
                let guard = current.0.clone();
                return Some(DashMapRefMulti::new(guard, v.1));
            }
        }

        if self.shard_i == self.map.shards().len() - 1 {
            return None;
        }

        self.shard_i += 1;
        let shards = self.map.shards();
        let guard = shards[self.shard_i].read();
        let sref: &HashMap<K, V> = unsafe {
            let p = &*guard as *const HashMap<K, V>;
            &*p
        };
        let iter = sref.iter();
        self.current = Some((Arc::new(guard), iter));

        self.next()
    }
}
