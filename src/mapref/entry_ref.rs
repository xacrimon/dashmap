use hashbrown::hash_table;

use super::one::RefMut;
use crate::lock::RwLockWriteGuardDetached;
use core::hash::Hash;
use std::mem;

/// Entry with a borrowed key.
pub enum EntryRef<'a, 'q, K, Q, V> {
    Occupied(OccupiedEntryRef<'a, 'q, K, Q, V>),
    Vacant(VacantEntryRef<'a, 'q, K, Q, V>),
}

impl<'a, 'q, K: Eq + Hash, Q, V> EntryRef<'a, 'q, K, Q, V> {
    /// Apply a function to the stored value if it exists.
    pub fn and_modify(self, f: impl FnOnce(&mut V)) -> Self {
        match self {
            EntryRef::Occupied(mut entry) => {
                f(entry.get_mut());

                EntryRef::Occupied(entry)
            }

            EntryRef::Vacant(entry) => EntryRef::Vacant(entry),
        }
    }
}

impl<'a, 'q, K: Eq + Hash + From<&'q Q>, Q, V> EntryRef<'a, 'q, K, Q, V> {
    /// Get the key of the entry.
    pub fn key(&self) -> &Q {
        match *self {
            EntryRef::Occupied(ref entry) => entry.key(),
            EntryRef::Vacant(ref entry) => entry.key(),
        }
    }

    /// Into the key of the entry.
    pub fn into_key(self) -> K {
        match self {
            EntryRef::Occupied(entry) => entry.into_key(),
            EntryRef::Vacant(entry) => entry.into_key(),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the default and return a mutable reference to that.
    pub fn or_default(self) -> RefMut<'a, K, V>
    where
        V: Default,
    {
        match self {
            EntryRef::Occupied(entry) => entry.into_ref(),
            EntryRef::Vacant(entry) => entry.insert(V::default()),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise a provided value and return a mutable reference to that.
    pub fn or_insert(self, value: V) -> RefMut<'a, K, V> {
        match self {
            EntryRef::Occupied(entry) => entry.into_ref(),
            EntryRef::Vacant(entry) => entry.insert(value),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the result of a provided function and return a mutable reference to that.
    pub fn or_insert_with(self, value: impl FnOnce() -> V) -> RefMut<'a, K, V> {
        match self {
            EntryRef::Occupied(entry) => entry.into_ref(),
            EntryRef::Vacant(entry) => entry.insert(value()),
        }
    }

    pub fn or_try_insert_with<E>(
        self,
        value: impl FnOnce() -> Result<V, E>,
    ) -> Result<RefMut<'a, K, V>, E> {
        match self {
            EntryRef::Occupied(entry) => Ok(entry.into_ref()),
            EntryRef::Vacant(entry) => Ok(entry.insert(value()?)),
        }
    }

    /// Sets the value of the entry, and returns a reference to the inserted value.
    pub fn insert(self, value: V) -> RefMut<'a, K, V> {
        match self {
            EntryRef::Occupied(mut entry) => {
                entry.insert(value);
                entry.into_ref()
            }
            EntryRef::Vacant(entry) => entry.insert(value),
        }
    }

    /// Sets the value of the entry, and returns an OccupiedEntryRef.
    ///
    /// If you are not interested in the occupied entry,
    /// consider [`insert`] as it doesn't need to clone the key.
    ///
    /// [`insert`]: EntryRef::insert
    pub fn insert_entry(self, value: V) -> OccupiedEntryRef<'a, 'q, K, Q, V>
    where
        K: Clone,
    {
        match self {
            EntryRef::Occupied(mut entry) => {
                entry.insert(value);
                entry
            }
            EntryRef::Vacant(entry) => entry.insert_entry(value),
        }
    }
}

pub struct VacantEntryRef<'a, 'q, K, Q, V> {
    shard: RwLockWriteGuardDetached<'a>,
    entry: hash_table::VacantEntry<'a, (K, V)>,
    key: &'q Q,
}

impl<'a, 'q, K: Eq + Hash, Q, V> VacantEntryRef<'a, 'q, K, Q, V> {
    pub(crate) fn new(
        shard: RwLockWriteGuardDetached<'a>,
        key: &'q Q,
        entry: hash_table::VacantEntry<'a, (K, V)>,
    ) -> Self {
        Self { shard, entry, key }
    }

    pub fn insert(self, value: V) -> RefMut<'a, K, V>
    where
        K: From<&'q Q>,
    {
        let k = K::from(self.key);
        let occupied = self.entry.insert((k, value));
        let (k, v) = occupied.into_mut();

        RefMut::new(self.shard, k, v)
    }

    /// Sets the value of the entry with the VacantEntryRef’s key, and returns an OccupiedEntry.
    pub fn insert_entry(self, value: V) -> OccupiedEntryRef<'a, 'q, K, Q, V>
    where
        K: From<&'q Q>,
    {
        let k = K::from(self.key);
        let entry = self.entry.insert((k, value));
        OccupiedEntryRef::new(self.shard, self.key, entry)
    }

    pub fn into_key(self) -> K
    where
        K: From<&'q Q>,
    {
        K::from(self.key)
    }

    pub fn key(&self) -> &'q Q {
        self.key
    }
}

pub struct OccupiedEntryRef<'a, 'q, K, Q, V> {
    shard: RwLockWriteGuardDetached<'a>,
    entry: hash_table::OccupiedEntry<'a, (K, V)>,
    key: &'q Q,
}

impl<'a, 'q, K: Eq + Hash, Q, V> OccupiedEntryRef<'a, 'q, K, Q, V> {
    pub(crate) fn new(
        shard: RwLockWriteGuardDetached<'a>,
        key: &'q Q,
        entry: hash_table::OccupiedEntry<'a, (K, V)>,
    ) -> Self {
        Self { shard, entry, key }
    }

    pub fn get(&self) -> &V {
        &self.entry.get().1
    }

    pub fn get_mut(&mut self) -> &mut V {
        &mut self.entry.get_mut().1
    }

    pub fn insert(&mut self, value: V) -> V {
        mem::replace(self.get_mut(), value)
    }

    pub fn into_ref(self) -> RefMut<'a, K, V> {
        let (k, v) = self.entry.into_mut();
        RefMut::new(self.shard, k, v)
    }

    pub fn into_key(self) -> K
    where
        K: From<&'q Q>,
    {
        K::from(self.key)
    }

    pub fn key(&self) -> &'q Q {
        self.key
    }

    pub fn remove(self) -> V {
        let ((_k, v), _) = self.entry.remove();
        v
    }

    pub fn remove_entry(self) -> (K, V) {
        let ((k, v), _) = self.entry.remove();
        (k, v)
    }

    pub fn replace_entry(self, value: V) -> (K, V)
    where
        K: From<&'q Q>,
    {
        let (k, v) = mem::replace(self.entry.into_mut(), (K::from(self.key), value));
        (k, v)
    }
}

#[cfg(test)]
mod tests {
    use equivalent::Equivalent;

    use crate::DashMap;

    use super::*;

    #[derive(Hash, PartialEq, Eq, Debug)]
    struct K(u32);
    impl From<&K> for u32 {
        fn from(value: &K) -> Self {
            value.0
        }
    }
    impl Equivalent<u32> for K {
        fn equivalent(&self, key: &u32) -> bool {
            self.0 == *key
        }
    }

    #[test]
    fn test_insert_into_vacant() {
        let map: DashMap<u32, u32> = DashMap::new();

        let entry = map.entry_ref(&K(1));

        assert!(matches!(entry, EntryRef::Vacant(_)));

        let val = entry.insert(2);

        assert_eq!(*val, 2);

        drop(val);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }

    #[test]
    fn test_insert_into_occupied() {
        let map: DashMap<u32, u32> = DashMap::new();

        map.insert(1, 1000);

        let entry = map.entry_ref(&K(1));

        assert!(matches!(&entry, EntryRef::Occupied(entry) if *entry.get() == 1000));

        let val = entry.insert(2);

        assert_eq!(*val, 2);

        drop(val);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }

    #[test]
    fn test_insert_entry_into_vacant() {
        let map: DashMap<u32, u32> = DashMap::new();

        let entry = map.entry_ref(&K(1));

        assert!(matches!(entry, EntryRef::Vacant(_)));

        let entry = entry.insert_entry(2);

        assert_eq!(*entry.get(), 2);

        drop(entry);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }

    #[test]
    fn test_insert_entry_into_occupied() {
        let map: DashMap<u32, u32> = DashMap::new();

        map.insert(1, 1000);

        let entry = map.entry_ref(&K(1));

        assert!(matches!(&entry, EntryRef::Occupied(entry) if *entry.get() == 1000));

        let entry = entry.insert_entry(2);

        assert_eq!(*entry.get(), 2);

        drop(entry);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }
}
