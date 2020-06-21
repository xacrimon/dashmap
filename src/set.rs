use crate::DashMap;
pub use iter_shim::Iter;
pub use key::KeyGuard;
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hash};
use std::iter::FromIterator;
use std::ops::Deref;

/// DashSet is a thin wrapper around [`DashMap`] using `()` as the value type. It uses
/// methods and types which are more convenient to work with on a set.
///
/// [`DashMap`]: struct.DashMap.html
pub struct DashSet<K, S = RandomState> {
    inner: DashMap<K, (), S>,
}

impl<K: Eq + Hash + 'static> DashSet<K, RandomState> {
    /// Creates a new DashSet with a capacity of 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    ///
    /// let fruits = DashSet::new();
    /// fruits.insert("Apples");
    /// fruits.insert("Pears");
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity_and_hasher(0, RandomState::new())
    }

    /// Creates a new DashSet with a specified starting capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    ///
    /// let numbers = DashSet::with_capacity(2);
    /// numbers.insert(2);
    /// numbers.insert(8);
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<K: Eq + Hash + 'static, S: BuildHasher + 'static> DashSet<K, S> {
    /// Creates a new DashSet with a capacity of 0 and the provided hasher.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let fruits = DashSet::new();
    /// fruits.insert("Apples");
    /// fruits.insert("Pears");
    /// ```
    #[inline]
    pub fn with_hasher(build_hasher: S) -> Self {
        Self::with_capacity_and_hasher(0, build_hasher)
    }

    /// Creates a new DashMap with a specified starting capacity and hasher.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let numbers = DashSet::with_capacity_and_hasher(2, s);
    /// numbers.insert(2);
    /// numbers.insert(8);
    /// ```
    #[inline]
    pub fn with_capacity_and_hasher(capacity: usize, build_hasher: S) -> Self {
        let inner = DashMap::with_capacity_and_hasher(capacity, build_hasher);

        Self { inner }
    }

    /// Inserts a key into the set. Returns true if the key was not already in the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    ///
    /// let set = DashSet::new();
    /// set.insert("I am the key!");
    /// ```
    #[inline]
    pub fn insert(&self, key: K) -> bool {
        self.inner.insert(key, ())
    }

    /// Check if the set contains a specific key.
    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.inner.contains_key(key)
    }

    /// Removes an entry from the set.
    /// Returns true if the key existed and the entry was removed. Otherwise returns false.
    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.inner.remove(key)
    }

    /// Create an iterator over all keys in the set.
    /// This does not take a snapshot of the set and thus changes
    /// during the lifetime of the iterator may or may not become visible in the iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    ///
    /// let words = DashSet::new();
    /// words.insert("hello");
    /// words.insert("world");
    /// words.insert("macn");
    /// words.insert("cheese");
    /// assert_eq!(words.iter().count(), 4);
    /// ```
    pub fn iter(&self) -> Iter<K> {
        Iter::new(self.inner.iter())
    }

    /// Retain keys that the filter closure returns true for.
    #[inline]
    pub fn retain(&self, mut predicate: impl FnMut(&K) -> bool) {
        self.inner.retain(|k, _| predicate(k))
    }

    /// Clear all keys in the set.
    #[inline]
    pub fn clear(&self) {
        self.inner.clear();
    }

    /// Get the amount of keys in the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    ///
    /// let words = DashSet::new();
    /// words.insert("hello");
    /// words.insert("world");
    /// words.insert("macn");
    /// words.insert("cheese");
    /// assert_eq!(words.len(), 4);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Checks if the set is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use dashmap::DashSet;
    ///
    /// let words = DashSet::<String>::new();
    /// assert_eq!(words.len(), 0);
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the capacity of the set. That is the maximum amount of keys before a reallocation is needed.
    /// The backend implementation cannot always know the capacity. If this function returns 0, the capacity is unknown.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Create a set from an iterator over keys.
    #[allow(clippy::should_implement_trait)]
    #[inline]
    pub fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = K>,
        S: Default,
    {
        let set = DashSet::with_hasher(S::default());

        for key in iter {
            set.insert(key);
        }

        set
    }

    /// Extend the set with an iterator over keys.
    #[inline]
    pub fn extend<T>(&self, iter: T)
    where
        T: IntoIterator<Item = K>,
    {
        for key in iter {
            self.insert(key);
        }
    }
}

impl<K: Eq + Hash + 'static> Default for DashSet<K, RandomState> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + 'static + fmt::Debug, S: BuildHasher + 'static> fmt::Debug for DashSet<K, S> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let guards: Vec<_> = self.iter().collect();
        f.debug_set()
            .entries(guards.iter().map(|guard| guard.deref()))
            .finish()
    }
}

impl<K: Eq + Hash + 'static> FromIterator<K> for DashSet<K, RandomState> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = K>,
    {
        Self::from_iter(iter)
    }
}

impl<K: Eq + Hash + 'static, S: BuildHasher + 'static> IntoIterator for &DashSet<K, S> {
    type Item = KeyGuard<K>;
    type IntoIter = Iter<K>;

    fn into_iter(self) -> Iter<K> {
        self.iter()
    }
}

pub mod iter_shim {
    use super::key::KeyGuard;
    use crate::iter_shim::Iter as MapIter;
    use std::hash::Hash;

    /// Iterator over keys in a set.
    pub struct Iter<K> {
        inner: MapIter<K, ()>,
    }

    impl<K: Eq + Hash> Iter<K> {
        pub(crate) fn new(inner: MapIter<K, ()>) -> Self {
            Self { inner }
        }
    }

    impl<K: Eq + Hash> Iterator for Iter<K> {
        type Item = KeyGuard<K>;

        #[inline(always)]
        fn next(&mut self) -> Option<Self::Item> {
            self.inner.next().map(|inner| KeyGuard { inner })
        }
    }
}

pub mod key {
    use crate::ElementGuard;
    use std::ops::Deref;

    /// `KeyGuard<K>`'s are references to active or past set keys.
    /// They exist to automatically manage memory across threads to
    /// ensure a safe interface.
    #[derive(Clone)]
    pub struct KeyGuard<K> {
        pub(crate) inner: ElementGuard<K, ()>,
    }

    impl<K> Deref for KeyGuard<K> {
        type Target = K;

        #[inline(always)]
        fn deref(&self) -> &Self::Target {
            self.inner.key()
        }
    }
}
