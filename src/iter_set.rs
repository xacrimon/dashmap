use crate::setref::multiple::RefMulti;
use core::hash::{BuildHasher, Hash};

pub struct OwningIter<K, S> {
    inner: crate::iter::OwningIter<K, (), S>,
}

impl<K: Eq + Hash, S: BuildHasher + Clone> OwningIter<K, S> {
    pub(crate) fn new(inner: crate::iter::OwningIter<K, (), S>) -> Self {
        Self { inner }
    }
}

impl<K: Eq + Hash, S: BuildHasher + Clone> Iterator for OwningIter<K, S> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, _)| k)
    }
}

pub struct Iter<'a, K> {
    inner: crate::iter::Iter<'a, K, ()>,
}

impl<'a, K: 'a + Eq + Hash> Iter<'a, K> {
    pub(crate) fn new(inner: crate::iter::Iter<'a, K, ()>) -> Self {
        Self { inner }
    }
}

impl<'a, K: 'a + Eq + Hash> Iterator for Iter<'a, K> {
    type Item = RefMulti<'a, K>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(RefMulti::new)
    }
}
