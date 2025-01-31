use crossbeam_utils::CachePadded;
use hashbrown::HashTable;

use super::multiple::{RefMulti, RefMutMulti};
use crate::lock::{RwLock, RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::table::ClashTable;
use core::slice;
use std::sync::Arc;

/// Iterator over a ClashTable yielding key value pairs.
///
/// # Examples
///
/// ```
/// use clashmap::ClashTable;
///
/// let map = ClashTable::new();
/// map.insert("hello", "world");
/// map.insert("alex", "steve");
/// let pairs: Vec<(&'static str, &'static str)> = map.into_iter().collect();
/// assert_eq!(pairs.len(), 2);
/// ```
pub struct OwningIter<T> {
    shards: std::vec::IntoIter<CachePadded<RwLock<HashTable<T>>>>,
    current: Option<GuardOwningIter<T>>,
}

impl<T> OwningIter<T> {
    pub(crate) fn new(map: ClashTable<T>) -> Self {
        Self {
            shards: map.shards.into_vec().into_iter(),
            current: None,
        }
    }
}

type GuardOwningIter<T> = hashbrown::hash_table::IntoIter<T>;

impl<T> Iterator for OwningIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some(t) = current.next() {
                    return Some(t);
                }
            }

            let shard = self.shards.next()?;
            let iter = shard.into_inner().into_inner().into_iter();

            self.current = Some(iter);
        }
    }
}

type GuardIter<'a, T> = (
    Arc<RwLockReadGuardDetached<'a>>,
    hashbrown::hash_table::Iter<'a, T>,
);

type GuardIterMut<'a, T> = (
    Arc<RwLockWriteGuardDetached<'a>>,
    hashbrown::hash_table::IterMut<'a, T>,
);

/// Iterator over a ClashTable yielding immutable references.
///
/// # Examples
///
/// ```
/// use clashmap::ClashTable;
///
/// let map = ClashTable::new();
/// map.insert("hello", "world");
/// assert_eq!(map.iter().count(), 1);
/// ```
pub struct Iter<'a, T> {
    shards: slice::Iter<'a, CachePadded<RwLock<HashTable<T>>>>,
    current: Option<GuardIter<'a, T>>,
}

impl<T> Clone for Iter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            shards: self.shards.clone(),
            current: self.current.clone(),
        }
    }
}

impl<'a, T: 'a> Iter<'a, T> {
    pub(crate) fn new(map: &'a ClashTable<T>) -> Self {
        Self {
            shards: map.shards.iter(),
            current: None,
        }
    }
}

impl<'a, T: 'a> Iterator for Iter<'a, T> {
    type Item = RefMulti<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some(t) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMulti::new(guard, t));
                }
            }

            let guard = self.shards.next()?.read();

            // SAFETY: we keep the guard alive with the shard iterator,
            // and with any refs produced by the iterator
            let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(guard) };
            self.current = Some((Arc::new(guard), shard.iter()));
        }
    }
}

/// Iterator over a ClashTable yielding mutable references.
///
/// # Examples
///
/// ```
/// use clashmap::ClashTable;
///
/// let map = ClashTable::new();
/// map.insert("Johnny", 21);
/// map.iter_mut().for_each(|mut r| *r += 1);
/// assert_eq!(*map.get("Johnny").unwrap(), 22);
/// ```
pub struct IterMut<'a, T> {
    shards: slice::Iter<'a, CachePadded<RwLock<HashTable<T>>>>,
    current: Option<GuardIterMut<'a, T>>,
}

impl<'a, T: 'a> IterMut<'a, T> {
    pub(crate) fn new(map: &'a ClashTable<T>) -> Self {
        Self {
            shards: map.shards.iter(),
            current: None,
        }
    }
}

impl<'a, T: 'a> Iterator for IterMut<'a, T> {
    type Item = RefMutMulti<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some(t) = current.1.next() {
                    let guard = current.0.clone();
                    return Some(RefMutMulti::new(guard, t));
                }
            }

            let guard = self.shards.next()?.write();

            // SAFETY: we keep the guard alive with the shard iterator,
            // and with any refs produced by the iterator
            let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(guard) };
            self.current = Some((Arc::new(guard), shard.iter_mut()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::hash::BuildHasher;
    use std::hash::RandomState;

    use crate::ClashTable;

    #[test]
    fn iter_mut_manual_count() {
        let map = ClashTable::new();
        let hasher = RandomState::new();

        map.entry(
            hasher.hash_one("Johnny"),
            |&t| t == "Johnny",
            |t| hasher.hash_one(t),
        )
        .or_insert("Johnny");

        assert_eq!(map.len(), 1);

        let mut c = 0;

        for shard in map.shards.iter() {
            c += shard.write().iter().count();
        }

        assert_eq!(c, 1);
    }

    #[test]
    fn into_iter_count() {
        let map = ClashTable::new();
        let hasher = RandomState::new();

        map.entry(
            hasher.hash_one("Johnny"),
            |&t| t == "Johnny",
            |t| hasher.hash_one(t),
        )
        .or_insert("Johnny");
        let c = map.into_iter().count();

        assert_eq!(c, 1);
    }

    #[test]
    fn iter_mut_count() {
        let map = ClashTable::new();
        let hasher = RandomState::new();

        map.entry(
            hasher.hash_one("Johnny"),
            |&t| t == "Johnny",
            |t| hasher.hash_one(t),
        )
        .or_insert("Johnny");

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter_mut().count(), 1);
    }

    #[test]
    fn iter_count() {
        let map = ClashTable::new();
        let hasher = RandomState::new();

        map.entry(
            hasher.hash_one("Johnny"),
            |&t| t == "Johnny",
            |t| hasher.hash_one(t),
        )
        .or_insert("Johnny");

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter().count(), 1);
    }

    #[test]
    fn iter_clone() {
        let map = ClashTable::new();
        let hasher = RandomState::new();

        map.entry(
            hasher.hash_one("Johnny"),
            |&t| t == "Johnny",
            |t| hasher.hash_one(t),
        )
        .or_insert("Johnny");
        map.entry(
            hasher.hash_one("Chucky"),
            |&t| t == "Chucky",
            |t| hasher.hash_one(t),
        )
        .or_insert("Chucky");

        let mut iter = map.iter();
        iter.next();

        let iter2 = iter.clone();

        assert_eq!(iter.count(), 1);
        assert_eq!(iter2.count(), 1);
    }
}
