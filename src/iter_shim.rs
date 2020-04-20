use crate::ElementGuard;
use std::hash::Hash;

pub struct Iter<K, V> {
    inner: Box<dyn Iterator<Item = ElementGuard<K, V>>>,
}

impl<K: Eq + Hash, V> Iter<K, V> {
    pub fn new(inner: Box<dyn Iterator<Item = ElementGuard<K, V>>>) -> Self {
        Self { inner }
    }
}

impl<K: Eq + Hash, V> Iterator for Iter<K, V> {
    type Item = ElementGuard<K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

#[cfg(test)]
mod tests {
    use crate::DashMap;

    #[test]
    fn iter_count() {
        let words = DashMap::new();
        words.insert("hello", "world");
        words.insert("macn", "cheese");
        assert_eq!(words.iter().count(), 2);
    }
}
