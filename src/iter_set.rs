use crate::setref::multiple::RefMulti;
use crate::t::Map;

pub struct OwningIter<K, S> {
    inner: crate::iter::OwningIter<K, (), S>,
}

impl<K, S> OwningIter<K, S> {
    pub(crate) fn new(inner: crate::iter::OwningIter<K, (), S>) -> Self {
        Self { inner }
    }
}

impl<K, S: Clone> Iterator for OwningIter<K, S> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, _)| k)
    }
}

unsafe impl<K, S> Send for OwningIter<K, S>
where
    K: Send,
    S: Send,
{
}

unsafe impl<K, S> Sync for OwningIter<K, S>
where
    K: Sync,
    S: Sync,
{
}

pub struct Iter<'a, K, S, M> {
    inner: crate::iter::Iter<'a, K, (), S, M>,
}

unsafe impl<'a, 'i, K, S, M> Send for Iter<'i, K, S, M>
where
    K: 'a + Send,
    S: 'a,
    M: Map<'a, K, (), S>,
{
}

unsafe impl<'a, 'i, K, S, M> Sync for Iter<'i, K, S, M>
where
    K: 'a + Sync,
    S: 'a,
    M: Map<'a, K, (), S>,
{
}

impl<'a, K, S: 'a, M: Map<'a, K, (), S>> Iter<'a, K, S, M> {
    pub(crate) fn new(inner: crate::iter::Iter<'a, K, (), S, M>) -> Self {
        Self { inner }
    }
}

impl<'a, K, S: 'a, M: Map<'a, K, (), S>> Iterator for Iter<'a, K, S, M> {
    type Item = RefMulti<'a, K, S>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(RefMulti::new)
    }
}
