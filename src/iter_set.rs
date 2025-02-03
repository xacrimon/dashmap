use crate::setref::multiple::RefMulti;
use core::hash::Hash;

pub struct OwningIter<K> {
    inner: crate::iter::OwningIter<K, ()>,
}

impl<K: Eq + Hash> OwningIter<K> {
    pub(crate) fn new(inner: crate::iter::OwningIter<K, ()>) -> Self {
        Self { inner }
    }
}

impl<K: Eq + Hash> Iterator for OwningIter<K> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, _)| k)
    }
}

pub struct Iter<'a, K> {
    inner: crate::iter::Iter<'a, K, ()>,
}

impl<'a, K: Eq + Hash + 'a> Iter<'a, K> {
    pub(crate) fn new(inner: crate::iter::Iter<'a, K, ()>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash + 'a> Iterator for Iter<'a, K> {
    type Item = RefMulti<'a, K>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(RefMulti::new)
    }
}
