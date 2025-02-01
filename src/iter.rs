use crossbeam_utils::CachePadded;
use hashbrown::hash_table;

use super::mapref::multiple::{RefMulti, RefMutMulti};
use crate::lock::{RwLock, RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::{DashMap, HashMap};
use core::hash::Hash;
use std::sync::Arc;

/// Iterator over a DashMap yielding key value pairs.
///
/// # Examples
///
/// ```
/// use dashmap::DashMap;
///
/// let map = DashMap::new();
/// map.insert("hello", "world");
/// map.insert("alex", "steve");
/// let pairs: Vec<(&'static str, &'static str)> = map.into_iter().collect();
/// assert_eq!(pairs.len(), 2);
/// ```
pub struct OwningIter<K, V> {
    shards: std::vec::IntoIter<CachePadded<RwLock<HashMap<K, V>>>>,
    current: Option<GuardOwningIter<K, V>>,
}

impl<K: Eq + Hash, V> OwningIter<K, V> {
    pub(crate) fn new<S>(map: DashMap<K, V, S>) -> Self {
        Self {
            shards: map.shards.into_vec().into_iter(),
            current: None,
        }
    }
}

type GuardOwningIter<K, V> = hash_table::IntoIter<(K, V)>;

impl<K: Eq + Hash, V> Iterator for OwningIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.next() {
                    return Some((k, v));
                }
            }

            let iter = self.shards.next()?.into_inner().into_inner().into_iter();
            self.current = Some(iter);
        }
    }
}

type GuardIter<'a, K, V> = (
    Arc<RwLockReadGuardDetached<'a>>,
    hash_table::Iter<'a, (K, V)>,
);

type GuardIterMut<'a, K, V> = (
    Arc<RwLockWriteGuardDetached<'a>>,
    hash_table::IterMut<'a, (K, V)>,
);

/// Iterator over a DashMap yielding immutable references.
///
/// # Examples
///
/// ```
/// use dashmap::DashMap;
///
/// let map = DashMap::new();
/// map.insert("hello", "world");
/// assert_eq!(map.iter().count(), 1);
/// ```
pub struct Iter<'a, K, V> {
    shards: std::slice::Iter<'a, CachePadded<RwLock<HashMap<K, V>>>>,
    current: Option<GuardIter<'a, K, V>>,
}

impl<'i, K: Clone + Hash + Eq, V: Clone> Clone for Iter<'i, K, V> {
    fn clone(&self) -> Self {
        Iter {
            shards: self.shards.clone(),
            current: self.current.clone(),
        }
    }
}

impl<'a, K: Eq + Hash + 'a, V: 'a> Iter<'a, K, V> {
    pub(crate) fn new<S>(map: &'a DashMap<K, V, S>) -> Self {
        Self {
            shards: map.shards.iter(),
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash + 'a, V: 'a> Iterator for Iter<'a, K, V> {
    type Item = RefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMulti::new(guard, k, v));
                }
            }

            let guard = self.shards.next()?.read();
            // SAFETY: we keep the guard alive with the shard iterator,
            // and with any refs produced by the iterator
            let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(guard) };

            let iter = shard.iter();

            self.current = Some((Arc::new(guard), iter));
        }
    }
}

/// Iterator over a DashMap yielding mutable references.
///
/// # Examples
///
/// ```
/// use dashmap::DashMap;
///
/// let map = DashMap::new();
/// map.insert("Johnny", 21);
/// map.iter_mut().for_each(|mut r| *r += 1);
/// assert_eq!(*map.get("Johnny").unwrap(), 22);
/// ```
pub struct IterMut<'a, K, V> {
    shards: std::slice::Iter<'a, CachePadded<RwLock<HashMap<K, V>>>>,
    current: Option<GuardIterMut<'a, K, V>>,
}

impl<'a, K: Eq + Hash + 'a, V: 'a> IterMut<'a, K, V> {
    pub(crate) fn new<S>(map: &'a DashMap<K, V, S>) -> Self {
        Self {
            shards: map.shards.iter(),
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash + 'a, V: 'a> Iterator for IterMut<'a, K, V> {
    type Item = RefMutMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMutMulti::new(guard, k, v));
                }
            }

            let guard = self.shards.next()?.write();

            // SAFETY: we keep the guard alive with the shard iterator,
            // and with any refs produced by the iterator
            let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(guard) };

            let iter = shard.iter_mut();

            self.current = Some((Arc::new(guard), iter));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::DashMap;

    #[test]
    fn iter_mut_manual_count() {
        let map = DashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        let mut c = 0;

        for shard in map.shards() {
            c += shard.write().iter().count();
        }

        assert_eq!(c, 1);
    }

    #[test]
    fn iter_mut_count() {
        let map = DashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter_mut().count(), 1);
    }

    #[test]
    fn iter_count() {
        let map = DashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter().count(), 1);
    }
}
