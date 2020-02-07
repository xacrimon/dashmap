use super::mapref::multiple::{RefMulti, RefMutMulti};
use super::util;
use crate::lock::{RwLockReadGuard, RwLockWriteGuard};
use crate::t::Map;
use crate::util::SharedValue;
use crate::HashMap;
use std::collections::hash_map;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;

type GuardIter<'a, K, V, S> = (
    Arc<RwLockReadGuard<'a, HashMap<K, V, S>>>,
    hash_map::Iter<'a, K, SharedValue<V>>,
);
type GuardIterMut<'a, K, V, S> = (
    Arc<RwLockWriteGuard<'a, HashMap<K, V, S>>>,
    hash_map::IterMut<'a, K, SharedValue<V>>,
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
pub struct Iter<'a, K, V, S, M> {
    map: &'a M,
    shard_i: usize,
    current: Option<GuardIter<'a, K, V, S>>,
}

unsafe impl<'a, K: Eq + Hash + Send, V: Send, S: 'a + BuildHasher, M: Map<'a, K, V, S>> Send
    for Iter<'a, K, V, S, M>
{
}
unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync, S: 'a + BuildHasher, M: Map<'a, K, V, S>>
    Sync for Iter<'a, K, V, S, M>
{
}

impl<'a, K: Eq + Hash, V, S: 'a + BuildHasher, M: Map<'a, K, V, S>> Iter<'a, K, V, S, M> {
    pub(crate) fn new(map: &'a M) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V, S: 'a + BuildHasher, M: Map<'a, K, V, S>> Iterator
    for Iter<'a, K, V, S, M>
{
    type Item = RefMulti<'a, K, V, S>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.as_mut() {
            if let Some((k, v)) = current.1.next() {
                let guard = current.0.clone();
                return Some(RefMulti::new(guard, k, v.get()));
            }
        }

        if self.shard_i == self.map._shard_count() {
            return None;
        }

        let guard = unsafe { self.map._yield_read_shard(self.shard_i) };
        let sref: &HashMap<K, V, S> = unsafe { util::change_lifetime_const(&*guard) };
        let iter = sref.iter();
        self.current = Some((Arc::new(guard), iter));
        self.shard_i += 1;

        self.next()
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
pub struct IterMut<'a, K, V, S, M> {
    map: &'a M,
    shard_i: usize,
    current: Option<GuardIterMut<'a, K, V, S>>,
}

unsafe impl<'a, K: Eq + Hash + Send, V: Send, S: 'a + BuildHasher, M: Map<'a, K, V, S>> Send
    for IterMut<'a, K, V, S, M>
{
}
unsafe impl<'a, K: Eq + Hash + Send + Sync, V: Send + Sync, S: 'a + BuildHasher, M: Map<'a, K, V, S>>
    Sync for IterMut<'a, K, V, S, M>
{
}

impl<'a, K: Eq + Hash, V, S: 'a + BuildHasher, M: Map<'a, K, V, S>> IterMut<'a, K, V, S, M> {
    pub(crate) fn new(map: &'a M) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V, S: 'a + BuildHasher, M: Map<'a, K, V, S>> Iterator
    for IterMut<'a, K, V, S, M>
{
    type Item = RefMutMulti<'a, K, V, S>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.as_mut() {
            if let Some((k, v)) = current.1.next() {
                let guard = current.0.clone();
                unsafe {
                    let k = util::change_lifetime_const(k);
                    let v = &mut *v.as_ptr();
                    return Some(RefMutMulti::new(guard, k, v));
                }
            }
        }

        if self.shard_i == self.map._shard_count() {
            return None;
        }

        let mut guard = unsafe { self.map._yield_write_shard(self.shard_i) };
        let sref: &mut HashMap<K, V, S> = unsafe { util::change_lifetime_mut(&mut *guard) };
        let iter = sref.iter_mut();
        self.current = Some((Arc::new(guard), iter));
        self.shard_i += 1;

        self.next()
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
            c += shard.write().iter_mut().count();
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
