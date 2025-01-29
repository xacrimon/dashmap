use crate::setref::multiple::RefMulti;
use core::hash::{BuildHasher, Hash};

pub struct OwningIter<K, S> {
    inner: crate::iter::OwningIter<K, (), S>,
}

impl<K: Eq + Hash, S: BuildHasher> OwningIter<K, S> {
    pub(crate) fn new(inner: crate::iter::OwningIter<K, (), S>) -> Self {
        Self { inner }
    }
}

impl<K: Eq + Hash, S: BuildHasher> Iterator for OwningIter<K, S> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, _)| k)
    }
}

pub struct Iter<'a, K> {
    inner: crate::iter::Iter<'a, K, ()>,
}

impl<K> Clone for Iter<'_, K> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
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

#[cfg(test)]
mod tests {
    use crate::ClashSet;

    #[test]
    fn into_iter_count() {
        let set = ClashSet::new();

        set.insert("Johnny");
        let c = set.into_iter().count();

        assert_eq!(c, 1);
    }

    #[test]
    fn iter_count() {
        let set = ClashSet::new();

        set.insert("Johnny");

        assert_eq!(set.len(), 1);

        assert_eq!(set.iter().count(), 1);
    }

    #[test]
    fn iter_clone() {
        let set = ClashSet::new();

        set.insert("Johnny");
        set.insert("Chucky");

        let mut iter = set.iter();
        iter.next();

        let iter2 = iter.clone();

        assert_eq!(iter.count(), 1);
        assert_eq!(iter2.count(), 1);
    }
}
