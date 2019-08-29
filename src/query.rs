use owning_ref::{OwningRef, OwningRefMut};

use crate::DashMap;
use std::borrow::Borrow;
use std::hash::Hash;

use crate::mapref::one::{DashMapRef, DashMapRefMut};

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
        Self { map }
    }

    pub fn get<'k, Q: Eq + Hash>(self, key: &'k Q) -> DashMapQueryGet<'a, 'k, Q, K, V>
    where
        K: Borrow<Q>,
    {
        DashMapQueryGet::new(self, key)
    }
}

// --

// -- QueryInsert

pub struct DashMapQueryInsert<'a, K: Eq + Hash, V> {
    inner: DashMapQuery<'a, K, V>,
    key: K,
    value: V,
}

impl<'a, K: Eq + Hash, V> DashMapQueryInsert<'a, K, V> {
    pub fn new(inner: DashMapQuery<'a, K, V>, key: K, value: V) -> Self {
        Self { inner, key, value }
    }

    pub fn sync(self) -> DashMapQueryInsertSync<'a, K, V> {
        DashMapQueryInsertSync::new(self)
    }
}

// --

// -- QueryInsertSync

pub struct DashMapQueryInsertSync<'a, K: Eq + Hash, V> {
    inner: DashMapQueryInsert<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> DashMapQueryInsertSync<'a, K, V> {
    pub fn new(inner: DashMapQueryInsert<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> DashMapExecutableQuery for DashMapQueryInsertSync<'a, K, V> {
    type Output = Option<(K, V)>;

    fn exec(self) -> Self::Output {
        let shard_id = self.inner.inner.map.determine_map(&self.inner.key);
        let shards = self.inner.inner.map.shards();
        let mut shard = shards[shard_id].write();
        let r = shard.remove_entry(&self.inner.key);
        shard.insert(self.inner.key, self.inner.value);
        r
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
        Self { inner, key }
    }

    pub fn sync(self) -> DashMapQueryGetSync<'a, 'k, Q, K, V> {
        DashMapQueryGetSync::new(self)
    }

    pub fn mutable(self) -> DashMapQueryGetMut<'a, 'k, Q, K, V> {
        DashMapQueryGetMut::new(self)
    }
}

// --

// -- QueryGetMut

pub struct DashMapQueryGetMut<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: DashMapQueryGet<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapQueryGetMut<'a, 'k, Q, K, V> {
    pub fn new(inner: DashMapQueryGet<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }

    pub fn sync(self) -> DashMapQueryGetMutSync<'a, 'k, Q, K, V> {
        DashMapQueryGetMutSync::new(self)
    }
}

// --

// -- QueryGetSync

pub struct DashMapQueryGetSync<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: DashMapQueryGet<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapQueryGetSync<'a, 'k, Q, K, V> {
    pub fn new(inner: DashMapQueryGet<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapExecutableQuery
    for DashMapQueryGetSync<'a, 'k, Q, K, V>
{
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

// -- QueryGetMutSync

pub struct DashMapQueryGetMutSync<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: DashMapQueryGetMut<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapQueryGetMutSync<'a, 'k, Q, K, V> {
    pub fn new(inner: DashMapQueryGetMut<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> DashMapExecutableQuery
    for DashMapQueryGetMutSync<'a, 'k, Q, K, V>
{
    type Output = Option<DashMapRefMut<'a, K, V>>;

    fn exec(self) -> Self::Output {
        let shard_id = self
            .inner
            .inner
            .inner
            .map
            .determine_map(&self.inner.inner.key);
        let shards = self.inner.inner.inner.map.shards();
        let shard = shards[shard_id].write();

        if shard.contains_key(&self.inner.inner.key) {
            let or = OwningRefMut::new(shard);
            let or = or.map_mut(|shard| shard.get_mut(self.inner.inner.key).unwrap());
            Some(DashMapRefMut::new(or))
        } else {
            None
        }
    }
}

// --
