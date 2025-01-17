use super::mapref::multiple::{RefMulti, RefMutMulti};
use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::DashMap;
use core::hash::{BuildHasher, Hash};
use core::mem;
use std::collections::hash_map::RandomState;
use std::marker::PhantomData;
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
pub struct OwningIter<K, V, S = RandomState> {
    map: DashMap<K, V, S>,
    shard_i: usize,
    current: Option<GuardOwningIter<K, V>>,
}

impl<K: Eq + Hash, V, S: BuildHasher + Clone> OwningIter<K, V, S> {
    pub(crate) fn new(map: DashMap<K, V, S>) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

type GuardOwningIter<K, V> = hashbrown::hash_table::IntoIter<(K, V)>;

impl<K: Eq + Hash, V, S: BuildHasher + Clone> Iterator for OwningIter<K, V, S> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.next() {
                    return Some((k, v));
                }
            }

            if self.shard_i == self.map.shards.len() {
                return None;
            }

            let mut shard_wl = unsafe { self.map.yield_write_shard(self.shard_i) };

            let map = mem::take(&mut *shard_wl);

            drop(shard_wl);

            let iter = map.into_iter();

            self.current = Some(iter);

            self.shard_i += 1;
        }
    }
}

type GuardIter<'a, K, V> = (
    Arc<RwLockReadGuardDetached<'a>>,
    hashbrown::hash_table::Iter<'a, (K, V)>,
);

type GuardIterMut<'a, K, V> = (
    Arc<RwLockWriteGuardDetached<'a>>,
    hashbrown::hash_table::IterMut<'a, (K, V)>,
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
pub struct Iter<'a, K, V, S = RandomState> {
    map: &'a DashMap<K, V, S>,
    shard_i: usize,
    current: Option<GuardIter<'a, K, V>>,
    marker: PhantomData<S>,
}

impl<K: Clone + Hash + Eq, V: Clone, S: Clone + BuildHasher> Clone for Iter<'_, K, V, S> {
    fn clone(&self) -> Self {
        Iter::new(self.map)
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: 'a + BuildHasher + Clone> Iter<'a, K, V, S> {
    pub(crate) fn new(map: &'a DashMap<K, V, S>) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
            marker: PhantomData,
        }
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: 'a + BuildHasher + Clone> Iterator for Iter<'a, K, V, S> {
    type Item = RefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMulti::new(guard, k, v));
                }
            }

            if self.shard_i == self.map.shards.len() {
                return None;
            }

            let guard = unsafe { self.map.yield_read_shard(self.shard_i) };
            let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(guard) };

            let iter = shard.iter();

            self.current = Some((Arc::new(guard), iter));

            self.shard_i += 1;
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
pub struct IterMut<'a, K, V, S = RandomState> {
    map: &'a DashMap<K, V, S>,
    shard_i: usize,
    current: Option<GuardIterMut<'a, K, V>>,
    marker: PhantomData<S>,
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: 'a + BuildHasher + Clone> IterMut<'a, K, V, S> {
    pub(crate) fn new(map: &'a DashMap<K, V, S>) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
            marker: PhantomData,
        }
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: 'a + BuildHasher + Clone> Iterator for IterMut<'a, K, V, S> {
    type Item = RefMutMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMutMulti::new(guard, k, v));
                }
            }

            if self.shard_i == self.map.shards.len() {
                return None;
            }

            let guard = unsafe { self.map.yield_write_shard(self.shard_i) };
            let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(guard) };

            let iter = shard.iter_mut();

            self.current = Some((Arc::new(guard), iter));

            self.shard_i += 1;
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

        for shard in map.shards().iter() {
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
