use crate::element::ElementGuard;
use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;

pub trait Table<K: Eq + Hash, V, S: BuildHasher> {
    type Iter: Iterator<Item = ElementGuard<K, V>> + Send + Sync;

    fn iter(&self) -> Self::Iter;
    fn new(capacity: usize, era: usize, build_hasher: Arc<S>) -> Self;
    fn insert_and_get(&self, key: K, value: V) -> ElementGuard<K, V>;
    fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>>;

    fn get<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash;

    fn contains_key<Q>(&self, search_key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.get(search_key).is_some()
    }

    fn remove_if_take<Q>(
        &self,
        search_key: &Q,
        predicate: &mut impl FnMut(&K, &V) -> bool,
    ) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash;

    fn remove_take<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.remove_if_take(search_key, &mut |_, _| true)
    }

    fn remove_if<Q>(&self, search_key: &Q, predicate: &mut impl FnMut(&K, &V) -> bool) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.remove_if_take(search_key, predicate).is_some()
    }

    fn remove<Q>(&self, search_key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.remove_if(search_key, &mut |_, _| true)
    }

    fn insert(&self, key: K, value: V) -> bool {
        self.replace(key, value).is_none()
    }

    fn extract<T, Q, F>(&self, search_key: &Q, do_extract: F) -> Option<T>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
        F: FnOnce(&K, &V) -> T,
    {
        self.get(search_key).map(|r| do_extract(r.key(), r.value()))
    }

    fn update_get<Q, F>(&self, search_key: &Q, do_update: &mut F) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V;

    fn update<Q, F>(&self, search_key: &Q, do_update: &mut F) -> bool
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        self.update_get(search_key, do_update).is_some()
    }

    fn retain(&self, predicate: &mut impl FnMut(&K, &V) -> bool);

    fn clear(&self) {
        self.retain(&mut |_, _| false);
    }

    fn len(&self) -> usize {
        self.iter().count()
    }

    fn capacity(&self) -> usize;
}
