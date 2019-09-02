use crate::mapref::one::{DashMapRef, DashMapRefMut};
use crate::DashMap;
use owning_ref::{OwningRef, OwningRefMut};
use std::borrow::Borrow;
use std::hash::Hash;

pub trait ExecutableQuery {
    type Output;

    fn exec(self) -> Self::Output;
}

// -- Query

pub struct Query<'a, K: Eq + Hash, V> {
    map: &'a DashMap<K, V>,
}

impl<'a, K: Eq + Hash, V> Query<'a, K, V> {
    pub fn new(map: &'a DashMap<K, V>) -> Self {
        map.transaction_lock().acquire_shared();
        Self { map }
    }

    pub fn insert(self, key: K, value: V) -> QueryInsert<'a, K, V> {
        QueryInsert::new(self, key, value)
    }

    pub fn get<'k, Q: Eq + Hash>(self, key: &'k Q) -> QueryGet<'a, 'k, Q, K, V>
    where
        K: Borrow<Q>,
    {
        QueryGet::new(self, key)
    }

    pub fn remove<'k, Q: Eq + Hash>(self, key: &'k Q) -> QueryRemove<'a, 'k, Q, K, V>
    where
        K: Borrow<Q>,
    {
        QueryRemove::new(self, key)
    }

    pub fn len(self) -> QueryLength<'a, K, V> {
        QueryLength::new(self)
    }

    pub fn clear(self) -> QueryClear<'a, K, V> {
        QueryClear::new(self)
    }
}

impl<'a, K: Eq + Hash, V> Drop for Query<'a, K, V> {
    fn drop(&mut self) {
        unsafe {
            self.map.transaction_lock().release_shared();
        }
    }
}

// --

// -- QueryClear

pub struct QueryClear<'a, K: Eq + Hash, V> {
    inner: Query<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryClear<'a, K, V> {
    pub fn new(inner: Query<'a, K, V>) -> Self {
        Self { inner }
    }

    pub fn sync(self) -> QueryClearSync<'a, K, V> {
        QueryClearSync::new(self)
    }
}

// --

// -- QueryClearSync

pub struct QueryClearSync<'a, K: Eq + Hash, V> {
    inner: QueryClear<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryClearSync<'a, K, V> {
    pub fn new(inner: QueryClear<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> ExecutableQuery for QueryClearSync<'a, K, V> {
    type Output = ();

    fn exec(self) -> Self::Output {
        let shards = self.inner.inner.map.shards();
        for shard in &**shards {
            shard.write().clear();
        }
    }
}

// --

// -- QueryLength

pub struct QueryLength<'a, K: Eq + Hash, V> {
    inner: Query<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryLength<'a, K, V> {
    pub fn new(inner: Query<'a, K, V>) -> Self {
        Self { inner }
    }

    pub fn sync(self) -> QueryLengthSync<'a, K, V> {
        QueryLengthSync::new(self)
    }
}

// --

// -- QueryLengthSync

pub struct QueryLengthSync<'a, K: Eq + Hash, V> {
    inner: QueryLength<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryLengthSync<'a, K, V> {
    pub fn new(inner: QueryLength<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> ExecutableQuery for QueryLengthSync<'a, K, V> {
    type Output = usize;

    fn exec(self) -> Self::Output {
        let shards = self.inner.inner.map.shards();
        let mut total = 0;
        for shard in &**shards {
            total += shard.read().len();
        }
        total
    }
}

// --

// -- QueryRemove

pub struct QueryRemove<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: Query<'a, K, V>,
    key: &'k Q,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryRemove<'a, 'k, Q, K, V> {
    pub fn new(inner: Query<'a, K, V>, key: &'k Q) -> Self {
        Self { inner, key }
    }

    pub fn sync(self) -> QueryRemoveSync<'a, 'k, Q, K, V> {
        QueryRemoveSync::new(self)
    }
}

// --

// -- QueryRemoveSync

pub struct QueryRemoveSync<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: QueryRemove<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryRemoveSync<'a, 'k, Q, K, V> {
    pub fn new(inner: QueryRemove<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> ExecutableQuery
    for QueryRemoveSync<'a, 'k, Q, K, V>
{
    type Output = Option<(K, V)>;

    fn exec(self) -> Self::Output {
        let shard_id = self.inner.inner.map.determine_map(&self.inner.key);
        let shards = self.inner.inner.map.shards();
        let mut shard = shards[shard_id].write();
        shard.remove_entry(&self.inner.key)
    }
}

// --

// -- QueryInsert

pub struct QueryInsert<'a, K: Eq + Hash, V> {
    inner: Query<'a, K, V>,
    key: K,
    value: V,
}

impl<'a, K: Eq + Hash, V> QueryInsert<'a, K, V> {
    pub fn new(inner: Query<'a, K, V>, key: K, value: V) -> Self {
        Self { inner, key, value }
    }

    pub fn sync(self) -> QueryInsertSync<'a, K, V> {
        QueryInsertSync::new(self)
    }
}

// --

// -- QueryInsertSync

pub struct QueryInsertSync<'a, K: Eq + Hash, V> {
    inner: QueryInsert<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryInsertSync<'a, K, V> {
    pub fn new(inner: QueryInsert<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> ExecutableQuery for QueryInsertSync<'a, K, V> {
    type Output = Option<V>;

    fn exec(self) -> Self::Output {
        let shard_id = self.inner.inner.map.determine_map(&self.inner.key);
        let shards = self.inner.inner.map.shards();
        let mut shard = shards[shard_id].write();
        shard.insert(self.inner.key, self.inner.value)
    }
}

// --

// -- QueryGet

pub struct QueryGet<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: Query<'a, K, V>,
    key: &'k Q,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryGet<'a, 'k, Q, K, V> {
    pub fn new(inner: Query<'a, K, V>, key: &'k Q) -> Self {
        Self { inner, key }
    }

    pub fn sync(self) -> QueryGetSync<'a, 'k, Q, K, V> {
        QueryGetSync::new(self)
    }

    pub fn mutable(self) -> QueryGetMut<'a, 'k, Q, K, V> {
        QueryGetMut::new(self)
    }
}

// --

// -- QueryGetMut

pub struct QueryGetMut<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: QueryGet<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryGetMut<'a, 'k, Q, K, V> {
    pub fn new(inner: QueryGet<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }

    pub fn sync(self) -> QueryGetMutSync<'a, 'k, Q, K, V> {
        QueryGetMutSync::new(self)
    }
}

// --

// -- QueryGetSync

pub struct QueryGetSync<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: QueryGet<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryGetSync<'a, 'k, Q, K, V> {
    pub fn new(inner: QueryGet<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> ExecutableQuery
    for QueryGetSync<'a, 'k, Q, K, V>
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

pub struct QueryGetMutSync<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: QueryGetMut<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryGetMutSync<'a, 'k, Q, K, V> {
    pub fn new(inner: QueryGetMut<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> ExecutableQuery
    for QueryGetMutSync<'a, 'k, Q, K, V>
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
