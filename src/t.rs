use dashmap_shard::HashMap;
use fxhash::FxBuildHasher;
use parking_lot::RwLock;
use std::borrow::Borrow;
use std::hash::Hash;
use crate::iter::{Iter, IterMut};
use crate::mapref::one::{Ref, RefMut};
use crate::mapref::entry::Entry;

pub trait Map<'a, K: 'a + Eq + Hash, V: 'a> {
    fn shards(&'a self) -> &'a [RwLock<HashMap<K, V, FxBuildHasher>>];
    fn insert(&self, key: K, value: V) -> Option<V>;
    fn remove<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn iter(&'a self) -> Iter<'a, K, V>;
    fn iter_mut(&'a self) -> IterMut<'a, K, V>;
    fn get<Q>(&'a self, key: &Q) -> Option<Ref<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn get_mut<Q>(&'a self, key: &Q) -> Option<RefMut<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn shrink_to_fit(&self);
    fn retain(&self, f: impl FnMut(&K, &mut V) -> bool);
    fn len(&self) -> usize;
    fn capacity(&self) -> usize;
    fn alter<Q>(&self, key: &Q, f: impl FnOnce(&K, V) -> V)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
    fn alter_all(&self, f: impl FnMut(&K, V) -> V);
    fn entry(&'a self, key: K) -> Entry<'a, K, V>;

    // provided

    fn clear(&self) {
        self.retain(|_, _| false)
    }

    fn contains_key<Q>(&'a self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized
    {
        self.get(key).is_some()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
