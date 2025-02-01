//! Central map trait to ease modifications and extensions down the road.

use crossbeam_utils::CachePadded;

use crate::iter::{Iter, IterMut};
use crate::lock::RwLock;
use crate::mapref::entry::Entry;
use crate::mapref::one::{Ref, RefMut};
use crate::try_result::TryResult;
use crate::HashMap;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};

/// Implementation detail that is exposed due to generic constraints in public types.
pub trait Map<'a, K: 'a + Eq + Hash, V: 'a, S: 'a + Clone + BuildHasher> {
    fn _shard_count(&self) -> usize;

    fn _shards(&self) -> &[CachePadded<RwLock<HashMap<K, V>>>];

    fn _insert(&self, key: K, value: V) -> Option<V>;

    fn _remove<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    fn _remove_if<Q>(&self, key: &Q, f: impl FnOnce(&K, &V) -> bool) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    fn _remove_if_mut<Q>(&self, key: &Q, f: impl FnOnce(&K, &mut V) -> bool) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    fn _iter(&'a self) -> Iter<'a, K, V, S, Self>
    where
        Self: Sized;

    fn _iter_mut(&'a self) -> IterMut<'a, K, V, S, Self>
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

    fn _try_get<Q>(&'a self, key: &Q) -> TryResult<Ref<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    fn _try_get_mut<Q>(&'a self, key: &Q) -> TryResult<RefMut<'a, K, V>>
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

    fn _view<Q, R>(&self, key: &Q, f: impl FnOnce(&K, &V) -> R) -> Option<R>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    fn _entry(&'a self, key: K) -> Entry<'a, K, V>;

    fn _try_entry(&'a self, key: K) -> Option<Entry<'a, K, V>>;

    fn _hasher(&self) -> S;

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
