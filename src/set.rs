use ahash::RandomState;
use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};
use std::iter::FromIterator;

use crate::DashMap;
use crate::setref::one::Ref;
use crate::iter_set::{Iter, OwningIter};

pub struct DashSet<K, S = RandomState> {
    inner: DashMap<K, (), S>
}

impl<K: Eq + Hash + Clone, S: Clone> Clone for DashSet<K, S> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }

    fn clone_from(&mut self, source: &Self) {
        self.inner.clone_from(&source.inner)
    }
}

impl<K, S> Default for DashSet<K, S>
where
    K: Eq + Hash,
    S: Default + BuildHasher + Clone,
{
    #[inline]
    fn default() -> Self {
        Self::with_hasher(Default::default())
    }
}

impl<'a, K: 'a + Eq + Hash> DashSet<K, RandomState> {
    #[inline]
    pub fn new() -> Self {
        Self::with_hasher(RandomState::default())
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::default())
    }
}

impl<'a, K: 'a + Eq + Hash, S: BuildHasher + Clone> DashSet<K, S> {
    #[inline]
    pub fn with_hasher(hasher: S) -> Self {
        Self::with_capacity_and_hasher(0, hasher)
    }
    #[inline]
    pub fn with_capacity_and_hasher(capacity: usize, hasher: S) -> Self {
        Self { inner: DashMap::with_capacity_and_hasher(capacity, hasher) }
    }
    // TODO: Self::shards()
    // TODO: Self::determine_map(key)
    // TODO: Self::determine_shard(hash)

    #[inline]
    pub fn insert(&self, key: K) -> bool {
        self.inner.insert(key, ()).is_none()
    }

    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> Option<K>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.inner.remove(key).map(|(k, _)| k)
    }

    #[inline]
    pub fn remove_if<Q>(&self, key: &Q, f: impl FnOnce(&K) -> bool) -> Option<K>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // TODO: Don't create another closure around f
        self.inner.remove_if(key, |k, _| f(k)).map(|(k, _)| k)
    }

    #[inline]
    pub fn iter(&'a self) -> Iter<'a, K, S, DashMap<K, (), S>> {
        let iter = self.inner.iter();
        Iter::new(iter)
    }

    #[inline]
    pub fn get<Q>(&'a self, key: &Q) -> Option<Ref<'a, K, S>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.inner.get(key).map(Ref::new)
    }

    #[inline]
    pub fn shrink_to_fit(&self) {
        self.inner.shrink_to_fit()
    }

    #[inline]
    pub fn retain(&self, mut f: impl FnMut(&K) -> bool) {
        // TODO: Don't create another closure
        self.inner.retain(|k, _| f(k))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn clear(&self) {
        self.inner.clear()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    #[inline]
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized
    {
        self.inner.contains_key(key)
    }
}

impl<'a, K: Eq + Hash, S: BuildHasher + Clone> IntoIterator for DashSet<K, S> {
    type Item = K;
    type IntoIter = OwningIter<K, S>;

    fn into_iter(self) -> Self::IntoIter {
        OwningIter::new(self.inner.into_iter())
    }
}

impl<K: Eq + Hash, S: BuildHasher + Clone> Extend<K> for DashSet<K, S> {
    fn extend<T: IntoIterator<Item = K>>(&mut self, iter: T) {
        let iter = iter.into_iter().map(|k| (k, ()));
        self.inner.extend(iter)
    }
}

impl<K: Eq + Hash> FromIterator<K> for DashSet<K, RandomState> {
    fn from_iter<I: IntoIterator<Item = K>>(iter: I) -> Self {
        let mut set = DashSet::new();
        set.extend(iter);
        set
    }
}

#[cfg(test)]
mod tests {
    use crate::DashSet;

    #[test]
    fn test_basic() {
        let set = DashSet::new();
        set.insert(0);
        assert_eq!(set.get(&0).as_deref(), Some(&0));
    }

    #[test]
    fn test_default() {
        let set: DashSet<u32> = DashSet::default();
        set.insert(0);
        assert_eq!(set.get(&0).as_deref(), Some(&0));
    }

    #[test]
    fn test_multiple_hashes() {
        let set = DashSet::<u32>::default();
        for i in 0..100 {
            assert!(set.insert(i));
        }
        for i in 0..100 {
            assert!(!set.insert(i));
        }
        for i in 0..100 {
            assert_eq!(Some(i), set.remove(&i));
        }
        for i in 0..100 {
            assert_eq!(None, set.remove(&i));
        }
    }
}
