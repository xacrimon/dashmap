//! Central map trait to ease modifications and extensions down the road.

use crate::iter::{Iter, IterMut};
use crate::mapref::entry::Entry;
use crate::mapref::one::{Ref, RefMut};
use crate::HashMap;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use std::borrow::Borrow;
use std::hash::Hash;

pub trait Map<'a, K: 'a + Eq + Hash, V: 'a> {
    fn _shard_count(&self) -> usize;
    unsafe fn _yield_read_shard(&'a self, i: usize) -> RwLockReadGuard<'a, HashMap<K, V>>;
    unsafe fn _yield_write_shard(&'a self, i: usize) -> RwLockWriteGuard<'a, HashMap<K, V>>;
    fn _insert(&self, key: K, value: V) -> Option<V>;
    fn _remove<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn _iter(&'a self) -> Iter<'a, K, V, Self>
    where
        Self: Sized;
    fn _iter_mut(&'a self) -> IterMut<'a, K, V, Self>
    where
        Self: Sized;
    fn _get<Q>(&'a self, key: &Q) -> Option<Ref<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn _get_mut<Q>(&'a self, key: &Q) -> Option<RefMut<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn _shrink_to_fit(&self);
    fn _retain(&self, f: impl FnMut(&K, &mut V) -> bool);
    fn _len(&self) -> usize;
    fn _capacity(&self) -> usize;
    fn _alter<Q>(&self, key: &Q, f: impl FnOnce(&K, V) -> V)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn _alter_all(&self, f: impl FnMut(&K, V) -> V);
    fn _entry(&'a self, key: K) -> Entry<'a, K, V>;

    // provided

    fn _clear(&self) {
        self._retain(|_, _| false)
    }

    fn _contains_key<Q>(&'a self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self._get(key).is_some()
    }

    fn _is_empty(&self) -> bool {
        self._len() == 0
    }
}
