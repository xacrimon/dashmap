use hashbrown::hash_table;

use super::one::RefMut;
use crate::lock::RwLockWriteGuardDetached;
use core::mem;

pub enum Entry<'a, T> {
    Occupied(OccupiedEntry<'a, T>),
    Vacant(VacantEntry<'a, T>),
}

impl<'a, T> Entry<'a, T> {
    /// Apply a function to the stored value if it exists.
    pub fn and_modify(self, f: impl FnOnce(&mut T)) -> Self {
        match self {
            Entry::Occupied(mut entry) => {
                f(entry.get_mut());

                Entry::Occupied(entry)
            }

            Entry::Vacant(entry) => Entry::Vacant(entry),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the default and return a mutable reference to that.
    pub fn or_default(self) -> RefMut<'a, T>
    where
        T: Default,
    {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(T::default()),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise a provided value and return a mutable reference to that.
    pub fn or_insert(self, value: T) -> RefMut<'a, T> {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(value),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the result of a provided function and return a mutable reference to that.
    pub fn or_insert_with(self, value: impl FnOnce() -> T) -> RefMut<'a, T> {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(value()),
        }
    }

    pub fn or_try_insert_with<E>(
        self,
        value: impl FnOnce() -> Result<T, E>,
    ) -> Result<RefMut<'a, T>, E> {
        match self {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => Ok(entry.insert(value()?)),
        }
    }

    /// Sets the value of the entry, and returns a reference to the inserted value.
    pub fn insert(self, value: T) -> RefMut<'a, T> {
        match self {
            Entry::Occupied(mut entry) => {
                entry.insert(value);
                entry.into_mut()
            }
            Entry::Vacant(entry) => entry.insert(value),
        }
    }

    /// Sets the value of the entry, and returns an OccupiedEntry.
    ///
    /// If you are not interested in the occupied entry,
    /// consider [`insert`] as it doesn't need to clone the key.
    ///
    /// [`insert`]: Entry::insert
    pub fn insert_entry(self, value: T) -> OccupiedEntry<'a, T> {
        match self {
            Entry::Occupied(mut entry) => {
                entry.insert(value);
                entry
            }
            Entry::Vacant(entry) => entry.insert_entry(value),
        }
    }
}

pub struct VacantEntry<'a, T> {
    guard: RwLockWriteGuardDetached<'a>,
    entry: hash_table::VacantEntry<'a, T>,
}

impl<'a, T> VacantEntry<'a, T> {
    pub(crate) fn new(
        guard: RwLockWriteGuardDetached<'a>,
        entry: hash_table::VacantEntry<'a, T>,
    ) -> Self {
        Self { guard, entry }
    }

    pub fn insert(self, value: T) -> RefMut<'a, T> {
        let occupied = self.entry.insert(value);

        RefMut::new(self.guard, occupied.into_mut())
    }

    /// Sets the value of the entry with the VacantEntry’s key, and returns an OccupiedEntry.
    pub fn insert_entry(self, value: T) -> OccupiedEntry<'a, T> {
        OccupiedEntry::new(self.guard, self.entry.insert(value))
    }
}

pub struct OccupiedEntry<'a, T> {
    guard: RwLockWriteGuardDetached<'a>,
    entry: hash_table::OccupiedEntry<'a, T>,
}

impl<'a, T> OccupiedEntry<'a, T> {
    pub(crate) fn new(
        guard: RwLockWriteGuardDetached<'a>,
        entry: hash_table::OccupiedEntry<'a, T>,
    ) -> Self {
        Self { guard, entry }
    }

    pub fn get(&self) -> &T {
        self.entry.get()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.entry.get_mut()
    }

    pub fn insert(&mut self, value: T) -> T {
        mem::replace(self.get_mut(), value)
    }

    pub fn into_mut(self) -> RefMut<'a, T> {
        RefMut::new(self.guard, self.entry.into_mut())
    }

    pub fn remove(self) -> T {
        let (t, _) = self.entry.remove();
        t
    }

    pub fn replace_entry(self, value: T) -> T {
        let t = mem::replace(self.entry.into_mut(), value);
        t
    }
}

#[cfg(test)]
mod tests {
    use std::hash::BuildHasher;
    use std::hash::RandomState;

    use crate::ClashTable;

    use super::*;

    #[test]
    fn test_insert_entry_into_vacant() {
        let map: ClashTable<(u32, u32)> = ClashTable::new();
        let hasher = RandomState::new();

        let entry = map.entry(
            hasher.hash_one(1),
            |&(t, _)| t == 1,
            |(t, _)| hasher.hash_one(t),
        );

        assert!(matches!(entry, Entry::Vacant(_)));

        let entry = entry.insert_entry((1, 2));

        assert_eq!(*entry.get(), (1, 2));

        drop(entry);

        assert_eq!(
            *map.find(hasher.hash_one(1), |&(t, _)| t == 1,).unwrap(),
            (1, 2)
        );
    }

    #[test]
    fn test_insert_entry_into_occupied() {
        let map: ClashTable<(u32, u32)> = ClashTable::new();
        let hasher = RandomState::new();

        {
            map.entry(
                hasher.hash_one(1),
                |&(t, _)| t == 1,
                |(t, _)| hasher.hash_one(t),
            )
            .or_insert((1, 1));
        }

        let entry = map.entry(
            hasher.hash_one(1),
            |&(t, _)| t == 1,
            |(t, _)| hasher.hash_one(t),
        );

        assert!(matches!(&entry, Entry::Occupied(entry) if *entry.get() == (1, 1)));

        let entry = entry.insert_entry((1, 2));

        assert_eq!(*entry.get(), (1, 2));

        drop(entry);

        assert_eq!(
            *map.find(hasher.hash_one(1), |&(t, _)| t == 1,).unwrap(),
            (1, 2)
        );
    }
}
