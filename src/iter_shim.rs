use crate::ElementGuard;
use std::hash::Hash;

pub struct Iter<'a, K, V> {
    inner: Box<dyn Iterator<Item = ElementGuard<K, V>> + Send + Sync + 'a>,
}

impl<'a, K: Eq + Hash, V> Iter<'a, K, V> {
    pub fn new(inner: Box<dyn Iterator<Item = ElementGuard<K, V>> + Send + Sync + 'a>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, V> Iterator for Iter<'a, K, V> {
    type Item = ElementGuard<K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
