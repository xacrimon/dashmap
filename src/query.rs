use super::iter::{Iter, IterMut};
use super::mapref::one::{DashMapRef, DashMapRefMut};
use super::util;
use super::DashMap;
use std::borrow::Borrow;
use std::hash::Hash;
use super::mapref::entry::{Entry, OccupiedEntry, VacantEntry};
use std::fmt;
use std::error;
use super::transaction::QueryAccessGuard;

pub trait ExecutableQuery {
    type Output;

    fn exec(self) -> Self::Output;
}

// -- Query

pub struct Query<'a, K: Eq + Hash, V> {
    map: &'a DashMap<K, V>,
    guard: QueryAccessGuard<'a>,
}

impl<'a, K: Eq + Hash, V> Query<'a, K, V> {
    pub fn new(map: &'a DashMap<K, V>, guard: QueryAccessGuard<'a>) -> Self {
        Self { map, guard }
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

    #[allow(clippy::wrong_self_convention)]
    pub fn is_empty(self) -> QueryIsEmpty<'a, K, V> {
        QueryIsEmpty::new(self)
    }

    pub fn iter(self) -> QueryIter<'a, K, V> {
        QueryIter::new(self)
    }

    pub fn iter_mut(self) -> QueryIterMut<'a, K, V> {
        QueryIterMut::new(self)
    }

    pub fn alter_all<F: FnMut(&K, V) -> V>(self, f: F) -> QueryAlterAll<'a, K, V, F> {
        QueryAlterAll::new(self, f)
    }

    pub fn swap<'k1, 'k2, Q: Eq + Hash, X: Eq + Hash>(
        self,
        key1: &'k1 Q,
        key2: &'k2 X,
    ) -> QuerySwap<'a, 'k1, 'k2, Q, X, K, V>
    where
        K: Borrow<Q> + Borrow<X>,
    {
        QuerySwap::new(self, key1, key2)
    }

    pub fn retain<F: FnMut(&K, &mut V) -> bool>(self, f: F) -> QueryRetain<'a, K, V, F> {
        QueryRetain::new(self, f)
    }

    pub fn contains<'k, Q: Eq + Hash>(self, key: &'k Q) -> QueryContains<'a, 'k, Q, K, V>
    where
        K: Borrow<Q>,
    {
        QueryContains::new(self, key)
    }

    pub fn entry(self, key: K) -> QueryEntry<'a, K, V> {
        QueryEntry::new(self, key)
    }
}

impl<'a, K: Eq + Hash, V> Drop for Query<'a, K, V> {
    fn drop(&mut self) {
        self.guard.destroy();
    }
}

// --

// -- QueryRetain

pub struct QueryRetain<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> {
    inner: Query<'a, K, V>,
    f: F,
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> QueryRetain<'a, K, V, F> {
    pub fn new(inner: Query<'a, K, V>, f: F) -> Self {
        Self { inner, f }
    }

    pub fn collect_discarded(self) -> QueryRetainCollect<'a, K, V, F> {
        QueryRetainCollect::new(self)
    }

    pub fn sync(self) -> QueryRetainSync<'a, K, V, F> {
        QueryRetainSync::new(self)
    }
}

// --

// -- QueryRetainCollect

pub struct QueryRetainCollect<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> {
    inner: QueryRetain<'a, K, V, F>,
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> QueryRetainCollect<'a, K, V, F> {
    pub fn new(inner: QueryRetain<'a, K, V, F>) -> Self {
        Self { inner }
    }

    pub fn sync(self) -> QueryRetainCollectSync<'a, K, V, F> {
        QueryRetainCollectSync::new(self)
    }
}

// --

// -- QueryRetainSync

pub struct QueryRetainSync<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> {
    inner: QueryRetain<'a, K, V, F>,
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> QueryRetainSync<'a, K, V, F> {
    pub fn new(inner: QueryRetain<'a, K, V, F>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> ExecutableQuery
    for QueryRetainSync<'a, K, V, F>
{
    type Output = ();

    fn exec(mut self) -> Self::Output {
        let shards = self.inner.inner.map.shards();

        for shard in &*shards {
            shard.write().retain(&mut self.inner.f);
        }
    }
}

// --

// -- QueryRetainCollectSync

pub struct QueryRetainCollectSync<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> {
    inner: QueryRetainCollect<'a, K, V, F>,
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> QueryRetainCollectSync<'a, K, V, F> {
    pub fn new(inner: QueryRetainCollect<'a, K, V, F>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, &mut V) -> bool> ExecutableQuery
    for QueryRetainCollectSync<'a, K, V, F>
{
    type Output = Vec<(K, V)>;

    fn exec(mut self) -> Self::Output {
        let shards = self.inner.inner.inner.map.shards();
        let mut discarded = Vec::new();

        for shard in &*shards {
            let mut shard = shard.write();
            let mut garbage: Vec<&K> = Vec::new();

            for (k, v) in &mut *shard {
                let keep = (self.inner.inner.f)(k, v);

                if !keep {
                    let k = unsafe { util::change_lifetime_const(k) };
                    garbage.push(k);
                }
            }

            for key in garbage {
                if let Some(e) = shard.remove_entry(key) {
                    discarded.push(e);
                }
            }
        }

        discarded
    }
}

// --

// -- QuerySwap

#[derive(Debug, PartialEq, Eq)]
pub enum QuerySwapError {
    InvalidKey,
}

impl fmt::Display for QuerySwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for QuerySwapError {}

pub struct QuerySwap<
    'a,
    'k1,
    'k2,
    Q: Eq + Hash,
    X: Eq + Hash,
    K: Eq + Hash + Borrow<Q> + Borrow<X>,
    V,
> {
    inner: Query<'a, K, V>,
    key1: &'k1 Q,
    key2: &'k2 X,
}

impl<'a, 'k1, 'k2, Q: Eq + Hash, X: Eq + Hash, K: Eq + Hash + Borrow<Q> + Borrow<X>, V>
    QuerySwap<'a, 'k1, 'k2, Q, X, K, V>
{
    pub fn new(inner: Query<'a, K, V>, key1: &'k1 Q, key2: &'k2 X) -> Self {
        Self { inner, key1, key2 }
    }

    pub fn sync(self) -> QuerySwapSync<'a, 'k1, 'k2, Q, X, K, V> {
        QuerySwapSync::new(self)
    }
}

// --

// -- QuerySwapSync

pub struct QuerySwapSync<
    'a,
    'k1,
    'k2,
    Q: Eq + Hash,
    X: Eq + Hash,
    K: Eq + Hash + Borrow<Q> + Borrow<X>,
    V,
> {
    inner: QuerySwap<'a, 'k1, 'k2, Q, X, K, V>,
}

impl<'a, 'k1, 'k2, Q: Eq + Hash, X: Eq + Hash, K: Eq + Hash + Borrow<Q> + Borrow<X>, V>
    QuerySwapSync<'a, 'k1, 'k2, Q, X, K, V>
{
    pub fn new(inner: QuerySwap<'a, 'k1, 'k2, Q, X, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, 'k1, 'k2, Q: Eq + Hash, X: Eq + Hash, K: Eq + Hash + Borrow<Q> + Borrow<X>, V>
    ExecutableQuery for QuerySwapSync<'a, 'k1, 'k2, Q, X, K, V>
{
    type Output = Result<(), QuerySwapError>;

    fn exec(self) -> Self::Output {
        let mut r1 = self
            .inner
            .inner
            .map
            .query()
            .get(self.inner.key1)
            .mutable()
            .sync()
            .exec().ok().ok_or(QuerySwapError::InvalidKey)?;
        let mut r2 = self
            .inner
            .inner
            .map
            .query()
            .get(self.inner.key2)
            .mutable()
            .sync()
            .exec().ok().ok_or(QuerySwapError::InvalidKey)?;
        unsafe { util::swap_nonoverlapping(r1.value_mut(), r2.value_mut()); }
        Ok(())
    }
}

// --

// -- QueryAlterAll

pub struct QueryAlterAll<'a, K: Eq + Hash, V, F: FnMut(&K, V) -> V> {
    inner: Query<'a, K, V>,
    f: F,
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, V) -> V> QueryAlterAll<'a, K, V, F> {
    pub fn new(inner: Query<'a, K, V>, f: F) -> Self {
        Self { inner, f }
    }

    pub fn sync(self) -> QueryAlterAllSync<'a, K, V, F> {
        QueryAlterAllSync::new(self)
    }
}

// --

// -- QueryAlterAllSync

pub struct QueryAlterAllSync<'a, K: Eq + Hash, V, F: FnMut(&K, V) -> V> {
    inner: QueryAlterAll<'a, K, V, F>,
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, V) -> V> QueryAlterAllSync<'a, K, V, F> {
    pub fn new(inner: QueryAlterAll<'a, K, V, F>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V, F: FnMut(&K, V) -> V> ExecutableQuery for QueryAlterAllSync<'a, K, V, F> {
    type Output = ();

    fn exec(mut self) -> Self::Output {
        self.inner
            .inner
            .map
            .query()
            .iter_mut()
            .exec()
            .for_each(|mut r| util::map_in_place_2(r.pair_mut(), &mut self.inner.f));
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

// -- QueryIter

pub struct QueryIter<'a, K: Eq + Hash, V> {
    inner: Query<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryIter<'a, K, V> {
    pub fn new(inner: Query<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> ExecutableQuery for QueryIter<'a, K, V> {
    type Output = Iter<'a, K, V>;

    fn exec(self) -> Self::Output {
        Iter::new(self.inner.map)
    }
}

// --

// -- QueryIterMut

pub struct QueryIterMut<'a, K: Eq + Hash, V> {
    inner: Query<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryIterMut<'a, K, V> {
    pub fn new(inner: Query<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> ExecutableQuery for QueryIterMut<'a, K, V> {
    type Output = IterMut<'a, K, V>;

    fn exec(self) -> Self::Output {
        IterMut::new(self.inner.map)
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
        for shard in &*shards {
            shard.write().clear();
        }
    }
}

// --

// -- QueryIsEmpty

pub struct QueryIsEmpty<'a, K: Eq + Hash, V> {
    inner: Query<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryIsEmpty<'a, K, V> {
    pub fn new(inner: Query<'a, K, V>) -> Self {
        Self { inner }
    }

    pub fn sync(self) -> QueryIsEmptySync<'a, K, V> {
        QueryIsEmptySync::new(self)
    }
}

// --

// -- QueryIsEmptySync

pub struct QueryIsEmptySync<'a, K: Eq + Hash, V> {
    inner: QueryIsEmpty<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryIsEmptySync<'a, K, V> {
    pub fn new(inner: QueryIsEmpty<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> ExecutableQuery for QueryIsEmptySync<'a, K, V> {
    type Output = bool;

    fn exec(self) -> Self::Output {
        self.inner.inner.map.query().len().sync().exec() == 0
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
        for shard in &*shards {
            total += shard.read().len();
        }
        total
    }
}

// --

// -- QueryRemove

#[derive(Debug, PartialEq, Eq)]
pub enum QueryRemoveError {
    InvalidKey,
}

impl fmt::Display for QueryRemoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for QueryRemoveError {}

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
    type Output = Result<(K, V), QueryRemoveError>;

    fn exec(self) -> Self::Output {
        let shard_id = self.inner.inner.map.determine_map(&self.inner.key).0;
        let shards = self.inner.inner.map.shards();
        let mut shard = shards[shard_id].write();
        shard.remove_entry(&self.inner.key).ok_or(QueryRemoveError::InvalidKey)
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
        let (shard_id, hash) = self.inner.inner.map.determine_map(&self.inner.key);
        let shards = self.inner.inner.map.shards();
        let mut shard = shards[shard_id].write();

        shard.insert_with_hash_nocheck(self.inner.key, self.inner.value, hash)
    }
}

// --

// -- QueryContains

pub struct QueryContains<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: Query<'a, K, V>,
    key: &'k Q,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryContains<'a, 'k, Q, K, V> {
    pub fn new(inner: Query<'a, K, V>, key: &'k Q) -> Self {
        Self { inner, key }
    }

    pub fn sync(self) -> QueryContainsSync<'a, 'k, Q, K, V> {
        QueryContainsSync::new(self)
    }
}

// --

// -- QueryContainsSync

pub struct QueryContainsSync<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> {
    inner: QueryContains<'a, 'k, Q, K, V>,
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> QueryContainsSync<'a, 'k, Q, K, V> {
    pub fn new(inner: QueryContains<'a, 'k, Q, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, 'k, Q: Eq + Hash, K: Eq + Hash + Borrow<Q>, V> ExecutableQuery
    for QueryContainsSync<'a, 'k, Q, K, V>
{
    type Output = bool;

    fn exec(self) -> Self::Output {
        self.inner
            .inner
            .map
            .query()
            .get(self.inner.key)
            .sync()
            .exec()
            .is_ok()
    }
}

// --

// -- QueryEntry

pub struct QueryEntry<'a, K: Eq + Hash, V> {
    inner: Query<'a, K, V>,
    key: K,
}

impl<'a, K: Eq + Hash, V> QueryEntry<'a, K, V> {
    pub fn new(inner: Query<'a, K, V>, key: K) -> Self {
        Self { inner, key }
    }

    pub fn sync(self) -> QueryEntrySync<'a, K, V> {
        QueryEntrySync::new(self)
    }
}

// --

// -- QueryEntrySync

pub struct QueryEntrySync<'a, K: Eq + Hash, V> {
    inner: QueryEntry<'a, K, V>,
}

impl<'a, K: Eq + Hash, V> QueryEntrySync<'a, K, V> {
    pub fn new(inner: QueryEntry<'a, K, V>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> ExecutableQuery for QueryEntrySync<'a, K, V> {
    type Output = Entry<'a, K, V>;

    fn exec(self) -> Self::Output {
        let shard_id = self.inner.inner.map.determine_map(&self.inner.key).0;
        let shards = self.inner.inner.map.shards();
        let shard = shards[shard_id].write();

        if shard.contains_key(&self.inner.key) {
            unsafe {
                let (k, v) = shard.get_key_value(&self.inner.key).unwrap();
                let k = util::change_lifetime_const(k);
                let v = util::change_lifetime_mut(util::to_mut(v));
                Entry::Occupied(OccupiedEntry::new(shard, Some(self.inner.key), (k, v)))
            }
        } else {
            Entry::Vacant(VacantEntry::new(shard, self.inner.key))
        }

        //unimplemented!()
    }
}

// --

// -- QueryGet

#[derive(Debug, PartialEq, Eq)]
pub enum QueryGetError {
    InvalidKey,
}

impl fmt::Display for QueryGetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for QueryGetError {}

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
    type Output = Result<DashMapRef<'a, K, V>, QueryGetError>;

    fn exec(self) -> Self::Output {
        let shard_id = self.inner.inner.map.determine_map(&self.inner.key).0;
        let shards = self.inner.inner.map.shards();
        let shard = shards[shard_id].read();
        if let Some((k, v)) = shard.get_key_value(&self.inner.key) {
            unsafe {
                let k = util::change_lifetime_const(k);
                let v = util::change_lifetime_const(v);
                return Ok(DashMapRef::new(shard, k, v));
            }
        }

        Err(QueryGetError::InvalidKey)
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
    type Output = Result<DashMapRefMut<'a, K, V>, QueryGetError>;

    fn exec(self) -> Self::Output {
        let shard_id = self
            .inner
            .inner
            .inner
            .map
            .determine_map(&self.inner.inner.key)
            .0;
        let shards = self.inner.inner.inner.map.shards();
        let shard = shards[shard_id].write();

        if let Some((k, v)) = shard.get_key_value(&self.inner.inner.key) {
            unsafe {
                let k = util::change_lifetime_const(k);
                let v = util::change_lifetime_mut(util::to_mut(v));
                return Ok(DashMapRefMut::new(shard, k, v));
            }
        }

        Err(QueryGetError::InvalidKey)
    }
}

// --
