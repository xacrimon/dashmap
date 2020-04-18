use crate::ElementGuard;
use std::hash::Hash;

pub struct Iter<K, V> {
    inner: Box<dyn Iterator<Item = ElementGuard<K, V>> + Send + Sync>,
}

impl<K: Eq + Hash, V> Iter<K, V> {
    pub fn new(inner: Box<dyn Iterator<Item = ElementGuard<K, V>> + Send + Sync>) -> Self {
        Self { inner }
    }
}

impl<K: Eq + Hash, V> Iterator for Iter<K, V> {
    type Item = ElementGuard<K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
