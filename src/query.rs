use hashbrown::HashMap;
use owning_ref::{OwningRef, OwningRefMut};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::ops::{Deref, DerefMut};
use std::hash::Hash;
use crate::DashMap;
use std::borrow::Borrow;
use std::marker::PhantomData;
use crate::mapref::one::DashMapRef;

pub trait DashMapExecutableQuery {
    type Output;

    fn exec(self) -> Self::Output;
}

// -- Query

pub struct DashMapQuery<'a, K: Eq + Hash, V> {
    map: &'a DashMap<K, V>,
}

impl<'a, K: Eq + Hash, V> DashMapQuery<'a, K, V> {
    pub fn new(map: &'a DashMap<K, V>) -> Self {
        Self {
            map
        }
    }

    pub fn get<'k, Q: Eq + Hash>(self, key: &'k Q) -> DashMapQueryGet<'a, 'k, Q, K, V>
    where
        K: Borrow<Q>,
    {
        DashMapQueryGet::new(self, key)
    }
}

// --

// -- QueryGet

pub struct DashMapQueryGet<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: DashMapQuery<'a, K, V>,
    key: &'k Q,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapQueryGet<'a, 'k, Q, K, V> {
    pub fn new(inner: DashMapQuery<'a, K, V>, key: &'k Q) -> Self {
        Self {
            inner,
            key,
        }
    }

    pub fn sync(self) -> DashMapQueryGetSync<'a, 'k, Q, K, V> {
        DashMapQueryGetSync::new(self)
    }
}

// --

// -- QueryGetSync

pub struct DashMapQueryGetSync<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: DashMapQueryGet<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapQueryGetSync<'a, 'k, Q, K, V> {
    pub fn new(inner: DashMapQueryGet<'a, 'k, Q, K, V>) -> Self {
        Self {
            inner
        }
    }
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapExecutableQuery for DashMapQueryGetSync<'a, 'k, Q, K, V> {
    type Output = Option<DashMapRef<'a, K, V>>;

    fn exec(self) -> Self::Output {
        let shard_id = self.inner.inner.map.determine_map(&self.inner.key);
        let shards = self.inner.inner.map.shards();
        let shard = shards[shard_id].read();

        if shard.contains_key(&self.inner.key) {
            let or = OwningRef::new(shard);
            let or = or.map(|shard| shard.get(self.inner.key).unwrap());
            Some(DashMapRef::new(or))
        } else {
            None
        }
    }
}

// --
