//! A concurrent hash set backed by DashMap.

use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};

use crate::DashMap;

/// A concurrent hash set backed by DashMap.
pub struct DashSet<T, S = RandomState> {
    map: DashMap<T, (), S>,
}

impl<'a, T: 'a + Eq + Hash> DashSet<T, RandomState> {
    /// Creates a new DashSet with a capacity of 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::set::DashSet;
    ///
    /// let games = DashSet::new();
    /// games.insert("Veloren");
    /// ```
    pub fn new() -> Self {
        Self {
            map: DashMap::with_hasher(RandomState::default()),
        }
    }
}

impl<'a, T: 'a + Eq + Hash, S: BuildHasher + Clone> DashSet<T, S> {
    /// Inserts a value into the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::set::DashSet;
    ///
    /// let set = DashSet::new();
    /// set.insert("I am the value!");
    /// ```
    pub fn insert(&self, value: T) -> bool {
        self.map.insert(value, ()).is_some()
    }

    /// Checks if the set contains a specific value.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::set::DashSet;
    ///
    /// let teams = DashSet::new();
    /// teams.insert("Dakota Cherries");
    /// assert!(teams.contains("Dakota Cherries"));
    /// ```
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::set::DashSet;

    #[test]
    fn test_basic() {
        let ds = DashSet::new();
        ds.insert(0);
        assert!(ds.contains(&0));
    }
}
