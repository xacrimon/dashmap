use super::mapref::multiple::{RefMulti, RefMutMulti};
use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::{DashMap, ShardIdx};
use core::hash::{BuildHasher, Hash};
use core::mem;
use std::collections::hash_map::RandomState;
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

            let shard = self.map.shards.get_mut(self.shard_i)?;
            let shard_wl = shard.get_mut();

            let map = mem::take(&mut *shard_wl);

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
pub struct Iter<'a, K, V> {
    shard_i: Option<ShardIdx<'a, K, V>>,
    current: Option<GuardIter<'a, K, V>>,
}

impl<K: Hash + Eq, V> Clone for Iter<'_, K, V> {
    fn clone(&self) -> Self {
        Self {
            shard_i: self.shard_i,
            current: self.current.clone(),
        }
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a> Iter<'a, K, V> {
    pub(crate) fn new<S: BuildHasher + Clone>(map: &'a DashMap<K, V, S>) -> Self {
        Self {
            shard_i: Some(map.first_shard()),
            current: None,
        }
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a> Iterator for Iter<'a, K, V> {
    type Item = RefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMulti::new(guard, k, v));
                }
            }

            let shard_i = self.shard_i.take()?;

            let guard = shard_i.yield_read_shard();
            let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(guard) };

            let iter = shard.iter();

            self.current = Some((Arc::new(guard), iter));

            self.shard_i = shard_i.next_shard();
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
    shard_i: Option<ShardIdx<'a, K, V>>,
    current: Option<GuardIterMut<'a, K, V>>,
}

impl<'a, K: 'a + Eq + Hash, V: 'a> IterMut<'a, K, V> {
    pub(crate) fn new<S: BuildHasher + Clone>(map: &'a DashMap<K, V, S>) -> Self {
        Self {
            shard_i: Some(map.first_shard()),
            current: None,
        }
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a> Iterator for IterMut<'a, K, V> {
    type Item = RefMutMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMutMulti::new(guard, k, v));
                }
            }

            let shard_i = self.shard_i.take()?;

            let guard = shard_i.yield_write_shard();
            let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(guard) };

            let iter = shard.iter_mut();

            self.current = Some((Arc::new(guard), iter));

            self.shard_i = shard_i.next_shard();
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

        for shard in map.shards.iter() {
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
