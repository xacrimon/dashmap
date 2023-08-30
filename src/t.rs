//! Central map trait to ease modifications and extensions down the road.

use crate::iter::{Iter, IterMut};
use crate::lock::{RwLockReadGuard, RwLockWriteGuard};
use crate::mapref::entry::Entry;
use crate::mapref::one::{Ref, RefMut};
use crate::try_result::TryResult;
use crate::HashMap;
use core::borrow::Borrow;
use core::hash::Hash;
use std::hash::BuildHasher;

/// Implementation detail that is exposed due to generic constraints in public types.
pub trait Map<'a, K: 'a, V: 'a, S: 'a> {
    fn _shard_count(&self) -> usize;

    /// # Safety
    ///
    /// The index must not be out of bounds.
    unsafe fn _get_read_shard(&'a self, i: usize) -> &'a HashMap<K, V, S>;

    /// # Safety
    ///
    /// The index must not be out of bounds.
    unsafe fn _yield_read_shard(&'a self, i: usize) -> RwLockReadGuard<'a, HashMap<K, V, S>>;

    /// # Safety
    ///
    /// The index must not be out of bounds.
    unsafe fn _yield_write_shard(&'a self, i: usize) -> RwLockWriteGuard<'a, HashMap<K, V, S>>;

    /// # Safety
    ///
    /// The index must not be out of bounds.
    unsafe fn _try_yield_read_shard(
        &'a self,
        i: usize,
    ) -> Option<RwLockReadGuard<'a, HashMap<K, V, S>>>;

    /// # Safety
    ///
    /// The index must not be out of bounds.
    unsafe fn _try_yield_write_shard(
        &'a self,
        i: usize,
    ) -> Option<RwLockWriteGuard<'a, HashMap<K, V, S>>>;

    fn _insert(&self, key: K, value: V) -> Option<V>
    where
        K: Hash + Eq,
        S: BuildHasher;

    fn _remove<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _remove_if<Q>(&self, key: &Q, f: impl FnOnce(&K, &V) -> bool) -> Option<(K, V)>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _remove_if_mut<Q>(&self, key: &Q, f: impl FnOnce(&K, &mut V) -> bool) -> Option<(K, V)>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _iter(&'a self) -> Iter<'a, K, V, S, Self>
    where
        Self: Sized;

    fn _iter_mut(&'a self) -> IterMut<'a, K, V, S, Self>
    where
        Self: Sized;

    fn _get<Q>(&'a self, key: &Q) -> Option<Ref<'a, K, V, S>>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _get_mut<Q>(&'a self, key: &Q) -> Option<RefMut<'a, K, V, S>>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _try_get<Q>(&'a self, key: &Q) -> TryResult<Ref<'a, K, V, S>>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _try_get_mut<Q>(&'a self, key: &Q) -> TryResult<RefMut<'a, K, V, S>>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _shrink_to_fit(&self)
    where
        K: Hash + Eq,
        S: BuildHasher;

    fn _retain(&self, f: impl FnMut(&K, &mut V) -> bool);

    fn _len(&self) -> usize;

    fn _capacity(&self) -> usize;

    fn _alter<Q>(&self, key: &Q, f: impl FnOnce(&K, V) -> V)
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _alter_all(&self, f: impl FnMut(&K, V) -> V);

    fn _view<Q, R>(&self, key: &Q, f: impl FnOnce(&K, &V) -> R) -> Option<R>
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher;

    fn _entry(&'a self, key: K) -> Entry<'a, K, V, S>
    where
        K: Hash + Eq,
        S: BuildHasher;

    fn _try_entry(&'a self, key: K) -> Option<Entry<'a, K, V, S>>
    where
        K: Hash + Eq,
        S: BuildHasher;

    fn _hasher(&self) -> S
    where
        S: Clone;

    // provided
    fn _clear(&self) {
        self._retain(|_, _| false)
    }

    fn _contains_key<Q>(&'a self, key: &Q) -> bool
    where
        K: Hash + Eq + Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        S: BuildHasher,
    {
        self._get(key).is_some()
    }

    fn _is_empty(&self) -> bool {
        self._len() == 0
    }
}
