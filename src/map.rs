use crate::raw::{self, InsertResult};
use seize::{Collector, Guard, LocalGuard, OwnedGuard};

use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::iter::FromIterator;

/// A concurrent hash table.
///
/// Most hash table operations require a [`Guard`](crate::Guard), which can be acquired through
/// [`HashMap::guard`] or using the [`HashMap::pin`] API. See the [crate-level documentation](crate#usage)
/// for details.
pub struct HashMap<K, V, S = RandomState> {
    raw: raw::HashMap<K, V, S>,
}

// Safety: We only ever hand out &K/V through shared references to the map,
// so normal Send/Sync rules apply. We never expose owned or mutable references
// to keys or values.
unsafe impl<K: Send, V: Send, S: Send> Send for HashMap<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for HashMap<K, V, S> {}

/// A builder for a [`HashMap`].
///
/// # Examples
///
/// ```rust
/// use papaya::{HashMap, ResizeMode};
/// use seize::Collector;
/// use std::collections::hash_map::RandomState;
///
/// let map: HashMap<i32, i32> = HashMap::builder()
///     // Set the initial capacity.
///     .capacity(2048)
///     // Set the hasher.
///     .hasher(RandomState::new())
///     // Set the resize mode.
///     .resize_mode(ResizeMode::Blocking)
///     // Set a custom garbage collector.
///     .collector(Collector::new().batch_size(128))
///     // Construct the hash map.
///     .build();
/// ```
pub struct HashMapBuilder<K, V, S = RandomState> {
    hasher: S,
    capacity: usize,
    collector: Collector,
    resize_mode: ResizeMode,
    _kv: PhantomData<(K, V)>,
}

impl<K, V> HashMapBuilder<K, V> {
    /// Set the hash builder used to hash keys.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and is designed
    /// to allow HashMaps to be resistant to attacks that cause many collisions
    /// and very poor performance. Setting it manually using this function can
    /// expose a DoS attack vector.
    ///
    /// The `hash_builder` passed should implement the [`BuildHasher`] trait for
    /// the HashMap to be useful, see its documentation for details.
    pub fn hasher<S>(self, hasher: S) -> HashMapBuilder<K, V, S> {
        HashMapBuilder {
            hasher,
            capacity: self.capacity,
            collector: self.collector,
            resize_mode: self.resize_mode,
            _kv: PhantomData,
        }
    }
}

impl<K, V, S> HashMapBuilder<K, V, S> {
    /// Set the initial capacity of the map.
    ///
    /// The table should be able to hold at least `capacity` elements before resizing.
    /// However, the capacity is an estimate, and the table may prematurely resize due
    /// to poor hash distribution. If `capacity` is 0, the hash map will not allocate.
    pub fn capacity(self, capacity: usize) -> HashMapBuilder<K, V, S> {
        HashMapBuilder {
            capacity,
            hasher: self.hasher,
            collector: self.collector,
            resize_mode: self.resize_mode,
            _kv: PhantomData,
        }
    }

    /// Set the resizing mode of the map. See [`ResizeMode`] for details.
    pub fn resize_mode(self, resize_mode: ResizeMode) -> Self {
        HashMapBuilder {
            resize_mode,
            hasher: self.hasher,
            capacity: self.capacity,
            collector: self.collector,
            _kv: PhantomData,
        }
    }

    /// Set the [`seize::Collector`] used for garbage collection.
    ///
    /// This method may be useful when you want more control over garbage collection.
    ///
    /// Note that all `Guard` references used to access the map must be produced by
    /// the provided `collector`.
    pub fn collector(self, collector: Collector) -> Self {
        HashMapBuilder {
            collector,
            hasher: self.hasher,
            capacity: self.capacity,
            resize_mode: self.resize_mode,
            _kv: PhantomData,
        }
    }

    /// Construct a [`HashMap`] from the builder, using the configured options.
    pub fn build(self) -> HashMap<K, V, S> {
        HashMap {
            raw: raw::HashMap::new(self.capacity, self.hasher, self.collector, self.resize_mode),
        }
    }
}

impl<K, V, S> fmt::Debug for HashMapBuilder<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashMapBuilder")
            .field("capacity", &self.capacity)
            .field("collector", &self.collector)
            .field("resize_mode", &self.resize_mode)
            .finish()
    }
}

/// Resize behavior for a [`HashMap`].
///
/// Hash maps must resize when the underlying table becomes full, migrating all key and value pairs
/// to a new table. This type allows you to configure the resizing behavior when passed to
/// [`HashMapBuilder::resize_mode`].
#[derive(Debug)]
pub enum ResizeMode {
    /// Writers copy a constant number of key/value pairs to the new table before making
    /// progress.
    ///
    /// Incremental resizes avoids latency spikes that can occur when insert operations have
    /// to resize a large table. However, they reduce parallelism during the resize and so can reduce
    /// overall throughput. Incremental resizing also means reads or write operations during an
    /// in-progress resize may have to search both the current and new table before succeeding, trading
    /// off median latency during a resize for tail latency.
    ///
    /// This is the default resize mode, with a chunk size of `64`.
    Incremental(usize),
    /// All writes to the map must wait till the resize completes before making progress.
    ///
    /// Blocking resizes tend to be better in terms of throughput, especially in setups with
    /// multiple writers that can perform the resize in parallel. However, they can lead to latency
    /// spikes for insert operations that have to resize large tables.
    ///
    /// If insert latency is not a concern, such as if the keys in your map are stable, enabling blocking
    /// resizes may yield better performance.
    ///
    /// Blocking resizing may also be a better option if you rely heavily on iteration or similar
    /// operations, as they require completing any in-progress resizes for consistency.
    Blocking,
}

impl Default for ResizeMode {
    fn default() -> Self {
        // Incremental resizing is a good default for most workloads as it avoids
        // unexpected latency spikes.
        ResizeMode::Incremental(64)
    }
}

impl<K, V> HashMap<K, V> {
    /// Creates an empty `HashMap`.
    ///
    /// The hash map is initially created with a capacity of 0, so it will not allocate
    /// until it is first inserted into.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    /// let map: HashMap<&str, i32> = HashMap::new();
    /// ```
    pub fn new() -> HashMap<K, V> {
        HashMap::with_capacity_and_hasher(0, RandomState::new())
    }

    /// Creates an empty `HashMap` with the specified capacity.
    ///
    /// The table should be able to hold at least `capacity` elements before resizing.
    /// However, the capacity is an estimate, and the table may prematurely resize due
    /// to poor hash distribution. If `capacity` is 0, the hash map will not allocate.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    /// let map: HashMap<&str, i32> = HashMap::with_capacity(10);
    /// ```
    pub fn with_capacity(capacity: usize) -> HashMap<K, V> {
        HashMap::with_capacity_and_hasher(capacity, RandomState::new())
    }

    /// Returns a builder for a `HashMap`.
    ///
    /// The builder can be used for more complex configuration, such as using
    /// a custom [`Collector`], or [`ResizeMode`].
    pub fn builder() -> HashMapBuilder<K, V> {
        HashMapBuilder {
            capacity: 0,
            hasher: RandomState::default(),
            collector: Collector::new(),
            resize_mode: ResizeMode::default(),
            _kv: PhantomData,
        }
    }
}

impl<K, V, S> Default for HashMap<K, V, S>
where
    S: Default,
{
    fn default() -> Self {
        HashMap::with_hasher(S::default())
    }
}

impl<K, V, S> HashMap<K, V, S> {
    /// Creates an empty `HashMap` which will use the given hash builder to hash
    /// keys.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and is designed
    /// to allow HashMaps to be resistant to attacks that cause many collisions
    /// and very poor performance. Setting it manually using this function can
    /// expose a DoS attack vector.
    ///
    /// The `hash_builder` passed should implement the [`BuildHasher`] trait for
    /// the HashMap to be useful, see its documentation for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    /// use std::hash::RandomState;
    ///
    /// let s = RandomState::new();
    /// let map = HashMap::with_hasher(s);
    /// map.pin().insert(1, 2);
    /// ```
    pub fn with_hasher(hash_builder: S) -> HashMap<K, V, S> {
        HashMap::with_capacity_and_hasher(0, hash_builder)
    }

    /// Creates an empty `HashMap` with at least the specified capacity, using
    /// `hash_builder` to hash the keys.
    ///
    /// The table should be able to hold at least `capacity` elements before resizing.
    /// However, the capacity is an estimate, and the table may prematurely resize due
    /// to poor hash distribution. If `capacity` is 0, the hash map will not allocate.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and is designed
    /// to allow HashMaps to be resistant to attacks that cause many collisions
    /// and very poor performance. Setting it manually using this function can
    /// expose a DoS attack vector.
    ///
    /// The `hasher` passed should implement the [`BuildHasher`] trait for
    /// the HashMap to be useful, see its documentation for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    /// use std::hash::RandomState;
    ///
    /// let s = RandomState::new();
    /// let map = HashMap::with_capacity_and_hasher(10, s);
    /// map.pin().insert(1, 2);
    /// ```
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> HashMap<K, V, S> {
        HashMap {
            raw: raw::HashMap::new(
                capacity,
                hash_builder,
                Collector::default(),
                ResizeMode::default(),
            ),
        }
    }

    /// Returns a pinned reference to the map.
    ///
    /// The returned reference manages a guard internally, preventing garbage collection
    /// for as long as it is held. See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn pin(&self) -> HashMapRef<'_, K, V, S, LocalGuard<'_>> {
        HashMapRef {
            guard: self.guard(),
            map: self,
        }
    }

    /// Returns a pinned reference to the map.
    ///
    /// Unlike [`HashMap::pin`], the returned reference implements `Send` and `Sync`,
    /// allowing it to be held across `.await` points in work-stealing schedulers.
    /// This is especially useful for iterators.
    ///
    /// The returned reference manages a guard internally, preventing garbage collection
    /// for as long as it is held. See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn pin_owned(&self) -> HashMapRef<'_, K, V, S, OwnedGuard<'_>> {
        HashMapRef {
            guard: self.owned_guard(),
            map: self,
        }
    }

    /// Returns a guard for use with this map.
    ///
    /// Note that holding on to a guard prevents garbage collection.
    /// See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn guard(&self) -> LocalGuard<'_> {
        self.raw.collector().enter()
    }

    /// Returns an owned guard for use with this map.
    ///
    /// Owned guards implement `Send` and `Sync`, allowing them to be held across
    /// `.await` points in work-stealing schedulers. This is especially useful
    /// for iterators.
    ///
    /// Note that holding on to a guard prevents garbage collection.
    /// See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn owned_guard(&self) -> OwnedGuard<'_> {
        self.raw.collector().enter_owned()
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Returns the number of entries in the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    ///
    /// map.pin().insert(1, "a");
    /// map.pin().insert(2, "b");
    /// assert!(map.len() == 2);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Returns `true` if the map is empty. Otherwise returns `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// assert!(map.is_empty());
    /// map.pin().insert("a", 1);
    /// assert!(!map.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the map contains a value for the specified key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// [`Eq`]: std::cmp::Eq
    /// [`Hash`]: std::hash::Hash
    ///
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// let m = map.pin();
    /// m.insert(1, "a");
    /// assert_eq!(m.contains_key(&1), true);
    /// assert_eq!(m.contains_key(&2), false);
    /// ```
    #[inline]
    pub fn contains_key<Q>(&self, key: &Q, guard: &impl Guard) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(key, guard).is_some()
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// [`Eq`]: std::cmp::Eq
    /// [`Hash`]: std::hash::Hash
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// let m = map.pin();
    /// m.insert(1, "a");
    /// assert_eq!(m.get(&1), Some(&"a"));
    /// assert_eq!(m.get(&2), None);
    /// ```
    #[inline]
    pub fn get<'g, Q>(&self, key: &Q, guard: &'g impl Guard) -> Option<&'g V>
    where
        K: Borrow<Q> + 'g,
        Q: Hash + Eq + ?Sized,
    {
        match self.raw.root(guard).get(key, guard) {
            Some((_, v)) => Some(v),
            None => None,
        }
    }

    /// Returns the key-value pair corresponding to the supplied key.
    ///
    /// The supplied key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// [`Eq`]: std::cmp::Eq
    /// [`Hash`]: std::hash::Hash
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// let m = map.pin();
    /// m.insert(1, "a");
    /// assert_eq!(m.get_key_value(&1), Some((&1, &"a")));
    /// assert_eq!(m.get_key_value(&2), None);
    /// ```
    #[inline]
    pub fn get_key_value<'g, Q>(&self, key: &Q, guard: &'g impl Guard) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.raw.root(guard).get(key, guard)
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, [`None`] is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned. The key is not updated, though; this matters for
    /// types that can be `==` without being identical. See the [standard library
    /// documentation] for details.
    ///
    /// [standard library documentation]: https://doc.rust-lang.org/std/collections/index.html#insert-and-complex-keys
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// assert_eq!(map.pin().insert(37, "a"), None);
    /// assert_eq!(map.pin().is_empty(), false);
    ///
    /// // note: you can also re-use a map pin like so:
    /// let m = map.pin();
    ///
    /// m.insert(37, "b");
    /// assert_eq!(m.insert(37, "c"), Some(&"b"));
    /// assert_eq!(m.get(&37), Some(&"c"));
    /// ```
    #[inline]
    pub fn insert<'g>(&self, key: K, value: V, guard: &'g impl Guard) -> Option<&'g V> {
        match self.raw.root(guard).insert(key, value, true, guard) {
            InsertResult::Inserted(_) => None,
            InsertResult::Replaced(value) => Some(value),
            InsertResult::Error { .. } => unreachable!(),
        }
    }

    /// Tries to insert a key-value pair into the map, and returns
    /// a reference to the value that was inserted.
    ///
    /// If the map already had this key present, nothing is updated, and
    /// an error containing the existing value is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// let m = map.pin();
    /// assert_eq!(m.try_insert(37, "a").unwrap(), &"a");
    ///
    /// let err = m.try_insert(37, "b").unwrap_err();
    /// assert_eq!(err.current, &"a");
    /// assert_eq!(err.not_inserted, "b");
    /// ```
    #[inline]
    pub fn try_insert<'g>(
        &self,
        key: K,
        value: V,
        guard: &'g impl Guard,
    ) -> Result<&'g V, OccupiedError<'g, V>> {
        match self.raw.root(guard).insert(key, value, false, guard) {
            InsertResult::Inserted(value) => Ok(value),
            InsertResult::Error {
                current,
                not_inserted,
            } => Err(OccupiedError {
                current,
                not_inserted,
            }),
            InsertResult::Replaced(_) => unreachable!(),
        }
    }

    /// Returns a reference to the value corresponding to the key, or inserts a default value.
    ///
    /// If the given key is present, the corresponding value is returned. If it is not present,
    /// the provided `value` is inserted, and a reference to the newly inserted value is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// assert_eq!(map.pin().get_or_insert("a", 3), &3);
    /// assert_eq!(map.pin().get_or_insert("a", 6), &3);
    /// ```
    #[inline]
    pub fn get_or_insert<'g>(&self, key: K, value: V, guard: &'g impl Guard) -> &'g V {
        // Note that we use `try_insert` instead of `compute` or `get_or_insert_with` here, as it
        // allows us to avoid the closure indirection.
        match self.try_insert(key, value, guard) {
            Ok(inserted) => inserted,
            Err(OccupiedError { current, .. }) => current,
        }
    }

    /// Returns a reference to the value corresponding to the key, or inserts a default value
    /// computed from a closure.
    ///
    /// If the given key is present, the corresponding value is returned. If it is not present,
    /// the value computed from `f` is inserted, and a reference to the newly inserted value is
    /// returned.
    ///
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// assert_eq!(map.pin().get_or_insert_with("a", || 3), &3);
    /// assert_eq!(map.pin().get_or_insert_with("a", || 6), &3);
    /// ```
    #[inline]
    pub fn get_or_insert_with<'g, F>(&self, key: K, f: F, guard: &'g impl Guard) -> &'g V
    where
        F: FnOnce() -> V,
        K: 'g,
    {
        self.raw.root(guard).get_or_insert_with(key, f, guard)
    }

    /// Updates an existing entry atomically.
    ///
    /// If the value for the specified `key` is present, the new value is computed and stored the
    /// using the provided update function, and the new value is returned. Otherwise, `None`
    /// is returned.
    ///
    ///
    /// The update function is given the current value associated with the given key and returns the
    /// new value to be stored. The operation is applied atomically only if the state of the entry remains
    /// the same, meaning that it is not concurrently modified in any way. If the entry is
    /// modified, the operation is retried with the new entry, similar to a traditional [compare-and-swap](https://en.wikipedia.org/wiki/Compare-and-swap)
    /// operation.
    ///
    /// Note that the `update` function should be pure as it may be called multiple times, and the output
    /// for a given entry may be memoized across retries.
    ///
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// map.pin().insert("a", 1);
    /// assert_eq!(map.pin().get(&"a"), Some(&1));
    ///
    /// map.pin().update("a", |v| v + 1);
    /// assert_eq!(map.pin().get(&"a"), Some(&2));
    /// ```
    #[inline]
    pub fn update<'g, F>(&self, key: K, update: F, guard: &'g impl Guard) -> Option<&'g V>
    where
        F: Fn(&V) -> V,
        K: 'g,
    {
        self.raw.root(guard).update(key, update, guard)
    }

    /// Updates an existing entry or inserts a default value.
    ///
    /// If the value for the specified `key` is present, the new value is computed and stored the
    /// using the provided update function, and the new value is returned. Otherwise, the provided
    /// `value` is inserted into the map, and a reference to the newly inserted value is returned.
    ///
    /// See [`HashMap::update`] for details about how atomic updates are performed.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// assert_eq!(*map.pin().update_or_insert("a", |i| i + 1, 0), 0);
    /// assert_eq!(*map.pin().update_or_insert("a", |i| i + 1, 0), 1);
    /// ```
    #[inline]
    pub fn update_or_insert<'g, F>(
        &self,
        key: K,
        update: F,
        value: V,
        guard: &'g impl Guard,
    ) -> &'g V
    where
        F: Fn(&V) -> V,
        K: 'g,
    {
        self.update_or_insert_with(key, update, || value, guard)
    }

    /// Updates an existing entry or inserts a default value computed from a closure.
    ///
    /// If the value for the specified `key` is present, the new value is computed and stored the
    /// using the provided update function, and the new value is returned. Otherwise, the value
    /// computed by `f` is inserted into the map, and a reference to the newly inserted value is
    /// returned.
    ///
    /// See [`HashMap::update`] for details about how atomic updates are performed.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// assert_eq!(*map.pin().update_or_insert_with("a", |i| i + 1, || 0), 0);
    /// assert_eq!(*map.pin().update_or_insert_with("a", |i| i + 1, || 0), 1);
    /// ```
    #[inline]
    pub fn update_or_insert_with<'g, U, F>(
        &self,
        key: K,
        update: U,
        f: F,
        guard: &'g impl Guard,
    ) -> &'g V
    where
        F: FnOnce() -> V,
        U: Fn(&V) -> V,
        K: 'g,
    {
        self.raw
            .root(guard)
            .update_or_insert_with(key, update, f, guard)
    }

    /// Updates an entry with a compare-and-swap (CAS) function.
    ///
    /// This method allows you to perform complex operations on the map atomically. The `compute`
    /// closure is given the current state of the entry and returns the operation that should be
    /// performed. The operation is applied atomically only if the state of the entry remains the same,
    /// meaning it is not concurrently modified in any way.
    ///
    /// Note that the `compute` function should be pure as it may be called multiple times, and
    /// the output for a given entry may be memoized across retries.
    ///
    /// In most cases you can avoid this method and instead use a higher-level atomic operation.
    /// See the [crate-level documentation](crate#atomic-operations) for details.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use papaya::{HashMap, Operation, Compute};
    ///
    /// let map = HashMap::new();
    /// let map = map.pin();
    ///
    /// let compute = |entry| match entry {
    ///     // Remove the value if it is even.
    ///     Some((_key, value)) if value % 2 == 0 => {
    ///         Operation::Remove
    ///     }
    ///
    ///     // Increment the value if it is odd.
    ///     Some((_key, value)) => {
    ///         Operation::Insert(value + 1)
    ///     }
    ///
    ///     // Do nothing if the key does not exist
    ///     None => Operation::Abort(()),
    /// };
    ///
    /// assert_eq!(map.compute('A', compute), Compute::Aborted(()));
    ///
    /// map.insert('A', 1);
    /// assert_eq!(map.compute('A', compute), Compute::Updated {
    ///     old: (&'A', &1),
    ///     new: (&'A', &2),
    /// });
    /// assert_eq!(map.compute('A', compute), Compute::Removed(&'A', &2));
    /// ```
    #[inline]
    pub fn compute<'g, F, T>(
        &self,
        key: K,
        compute: F,
        guard: &'g impl Guard,
    ) -> Compute<'g, K, V, T>
    where
        F: FnMut(Option<(&'g K, &'g V)>) -> Operation<V, T>,
    {
        self.raw.root(guard).compute(key, compute, guard)
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// map.pin().insert(1, "a");
    /// assert_eq!(map.pin().remove(&1), Some(&"a"));
    /// assert_eq!(map.pin().remove(&1), None);
    /// ```
    #[inline]
    pub fn remove<'g, Q>(&self, key: &Q, guard: &'g impl Guard) -> Option<&'g V>
    where
        K: Borrow<Q> + 'g,
        Q: Hash + Eq + ?Sized,
    {
        match self.raw.root(guard).remove(key, guard) {
            Some((_, value)) => Some(value),
            None => None,
        }
    }

    /// Removes a key from the map, returning the stored key and value if the
    /// key was previously in the map.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    /// map.pin().insert(1, "a");
    /// assert_eq!(map.pin().get(&1), Some(&"a"));
    /// assert_eq!(map.pin().remove_entry(&1), Some((&1, &"a")));
    /// assert_eq!(map.pin().remove(&1), None);
    /// ```
    #[inline]
    pub fn remove_entry<'g, Q>(&self, key: &Q, guard: &'g impl Guard) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.raw.root(guard).remove(key, guard)
    }

    /// Tries to reserve capacity for `additional` more elements to be inserted
    /// in the `HashMap`.
    ///
    /// After calling this method, the table should be able to hold at least `capacity` elements
    /// before resizing. However, the capacity is an estimate, and the table may prematurely resize
    /// due to poor hash distribution. The collection may also reserve more space to avoid frequent
    /// reallocations.
    ///
    /// # Panics
    ///
    /// Panics if the new allocation size overflows `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map: HashMap<&str, i32> = HashMap::new();
    /// map.pin().reserve(10);
    /// ```
    #[inline]
    pub fn reserve(&self, additional: usize, guard: &impl Guard) {
        self.raw.root(guard).reserve(additional, guard);
    }

    /// Clears the map, removing all key-value pairs.
    ///
    /// Note that this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::new();
    ///
    /// map.pin().insert(1, "a");
    /// map.pin().clear();
    /// assert!(map.pin().is_empty());
    /// ```
    #[inline]
    pub fn clear(&self, guard: &impl Guard) {
        self.raw.root(guard).clear(guard)
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all pairs `(k, v)` for which `f(&k, &v)` returns `false`.
    /// The elements are visited in unsorted (and unspecified) order.
    ///
    /// Note the function may be called more than once for a given key if its value is
    /// concurrently modified during removal.
    ///
    /// Additionally, this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let mut map: HashMap<i32, i32> = (0..8).map(|x| (x, x * 10)).collect();
    /// map.pin().retain(|&k, _| k % 2 == 0);
    /// assert_eq!(map.len(), 4);
    /// ```
    #[inline]
    pub fn retain<F>(&mut self, f: F, guard: &impl Guard)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.raw.root(guard).retain(f, guard)
    }

    /// An iterator visiting all key-value pairs in arbitrary order.
    /// The iterator element type is `(&K, &V)`.
    ///
    /// Note that this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::from([
    ///     ("a", 1),
    ///     ("b", 2),
    ///     ("c", 3),
    /// ]);
    ///
    /// for (key, val) in map.pin().iter() {
    ///     println!("key: {key} val: {val}");
    /// }
    #[inline]
    pub fn iter<'g, G>(&self, guard: &'g G) -> Iter<'g, K, V, G>
    where
        G: Guard,
    {
        Iter {
            raw: self.raw.root(guard).iter(guard),
        }
    }

    /// An iterator visiting all keys in arbitrary order.
    /// The iterator element type is `&K`.
    ///
    /// Note that this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::from([
    ///     ("a", 1),
    ///     ("b", 2),
    ///     ("c", 3),
    /// ]);
    ///
    /// for key in map.pin().keys() {
    ///     println!("{key}");
    /// }
    /// ```
    #[inline]
    pub fn keys<'g, G>(&self, guard: &'g G) -> Keys<'g, K, V, G>
    where
        G: Guard,
    {
        Keys {
            iter: self.iter(guard),
        }
    }

    /// An iterator visiting all values in arbitrary order.
    /// The iterator element type is `&V`.
    ///
    /// Note that this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashMap;
    ///
    /// let map = HashMap::from([
    ///     ("a", 1),
    ///     ("b", 2),
    ///     ("c", 3),
    /// ]);
    ///
    /// for value in map.pin().values() {
    ///     println!("{value}");
    /// }
    /// ```
    #[inline]
    pub fn values<'g, G>(&self, guard: &'g G) -> Values<'g, K, V, G>
    where
        G: Guard,
    {
        Values {
            iter: self.iter(guard),
        }
    }
}

/// An operation to perform on given entry in a [`HashMap`].
///
/// See [`HashMap::compute`] for details.
#[derive(Debug, PartialEq, Eq)]
pub enum Operation<V, T> {
    /// Insert the given value.
    Insert(V),

    /// Remove the entry from the map.
    Remove,

    /// Abort the operation with the given value.
    Abort(T),
}

/// The result of a [`compute`](HashMap::compute) operation.
///
/// Contains information about the [`Operation`] that was performed.
#[derive(Debug, PartialEq, Eq)]
pub enum Compute<'g, K, V, T> {
    /// The given entry was inserted.
    Inserted(&'g K, &'g V),

    /// The entry was updated.
    Updated {
        /// The entry that was replaced.
        old: (&'g K, &'g V),

        /// The entry that was inserted.
        new: (&'g K, &'g V),
    },

    /// The given entry was removed.
    Removed(&'g K, &'g V),

    /// The operation was aborted with the given value.
    Aborted(T),
}

/// An error returned by [`try_insert`](HashMap::try_insert) when the key already exists.
///
/// Contains the existing value, and the value that was not inserted.
#[derive(Debug, PartialEq, Eq)]
pub struct OccupiedError<'a, V: 'a> {
    /// The value in the map that was already present.
    pub current: &'a V,
    /// The value which was not inserted, because the entry was already occupied.
    pub not_inserted: V,
}

impl<K, V, S> PartialEq for HashMap<K, V, S>
where
    K: Hash + Eq,
    V: PartialEq,
    S: BuildHasher,
{
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }

        let (guard1, guard2) = (&self.guard(), &other.guard());

        let mut iter = self.iter(guard1);
        iter.all(|(key, value)| other.get(key, guard2).map_or(false, |v| *value == *v))
    }
}

impl<K, V, S> Eq for HashMap<K, V, S>
where
    K: Hash + Eq,
    V: Eq,
    S: BuildHasher,
{
}

impl<K, V, S> fmt::Debug for HashMap<K, V, S>
where
    K: Hash + Eq + fmt::Debug,
    V: fmt::Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let guard = self.guard();
        f.debug_map().entries(self.iter(&guard)).finish()
    }
}

impl<K, V, S> Extend<(K, V)> for &HashMap<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        // from `hashbrown::HashMap::extend`:
        // Keys may be already present or show multiple times in the iterator.
        // Reserve the entire hint lower bound if the map is empty.
        // Otherwise reserve half the hint (rounded up), so the map
        // will only resize twice in the worst case.
        let iter = iter.into_iter();
        let reserve = if self.is_empty() {
            iter.size_hint().0
        } else {
            (iter.size_hint().0 + 1) / 2
        };

        let guard = self.guard();
        self.reserve(reserve, &guard);

        for (key, value) in iter {
            self.insert(key, value, &guard);
        }
    }
}

impl<'a, K, V, S> Extend<(&'a K, &'a V)> for &HashMap<K, V, S>
where
    K: Copy + Hash + Eq,
    V: Copy,
    S: BuildHasher,
{
    fn extend<T: IntoIterator<Item = (&'a K, &'a V)>>(&mut self, iter: T) {
        self.extend(iter.into_iter().map(|(&key, &value)| (key, value)));
    }
}

impl<K, V, const N: usize> From<[(K, V); N]> for HashMap<K, V, RandomState>
where
    K: Hash + Eq,
{
    fn from(arr: [(K, V); N]) -> Self {
        HashMap::from_iter(arr)
    }
}

impl<K, V, S> FromIterator<(K, V)> for HashMap<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher + Default,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut iter = iter.into_iter();

        if let Some((key, value)) = iter.next() {
            let (lower, _) = iter.size_hint();
            let map = HashMap::with_capacity_and_hasher(lower.saturating_add(1), S::default());

            // Ideally we could use an unprotected guard here. However, `insert`
            // returns references to values that were replaced and retired, so
            // we need a "real" guard. A `raw_insert` method that strictly returns
            // pointers would fix this.
            {
                let map = map.pin();
                map.insert(key, value);
                for (key, value) in iter {
                    map.insert(key, value);
                }
            }

            map
        } else {
            Self::default()
        }
    }
}

impl<K, V, S> Clone for HashMap<K, V, S>
where
    K: Clone + Hash + Eq,
    V: Clone,
    S: BuildHasher + Clone,
{
    fn clone(&self) -> HashMap<K, V, S> {
        let other = HashMap::builder()
            .capacity(self.len())
            .hasher(self.raw.hasher.clone())
            .collector(self.raw.collector().clone())
            .build();

        {
            let (guard1, guard2) = (&self.guard(), &other.guard());
            for (key, value) in self.iter(guard1) {
                other.insert(key.clone(), value.clone(), guard2);
            }
        }

        other
    }
}

/// A pinned reference to a [`HashMap`].
///
/// This type is created with [`HashMap::pin`] and can be used to easily access a [`HashMap`]
/// without explicitly managing a guard. See the [crate-level documentation](crate#usage) for details.
pub struct HashMapRef<'map, K, V, S, G> {
    guard: G,
    map: &'map HashMap<K, V, S>,
}

impl<'map, K, V, S, G> HashMapRef<'map, K, V, S, G>
where
    K: Hash + Eq,
    S: BuildHasher,
    G: Guard,
{
    /// Returns a reference to the inner [`HashMap`].
    #[inline]
    pub fn map(&self) -> &'map HashMap<K, V, S> {
        self.map
    }

    /// Returns the number of entries in the map.
    ///
    /// See [`HashMap::len`] for details.
    #[inline]
    pub fn len(&self) -> usize {
        self.map.raw.len()
    }

    /// Returns `true` if the map is empty. Otherwise returns `false`.
    ///
    /// See [`HashMap::is_empty`] for details.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the map contains a value for the specified key.
    ///
    /// See [`HashMap::contains_key`] for details.
    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(key).is_some()
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// See [`HashMap::get`] for details.
    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self.root().get(key, &self.guard) {
            Some((_, v)) => Some(v),
            None => None,
        }
    }

    /// Returns the key-value pair corresponding to the supplied key.
    ///
    /// See [`HashMap::get_key_value`] for details.
    #[inline]
    pub fn get_key_value<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.root().get(key, &self.guard)
    }

    /// Inserts a key-value pair into the map.
    ///
    /// See [`HashMap::insert`] for details.
    #[inline]
    pub fn insert(&self, key: K, value: V) -> Option<&V> {
        match self.root().insert(key, value, true, &self.guard) {
            InsertResult::Inserted(_) => None,
            InsertResult::Replaced(value) => Some(value),
            InsertResult::Error { .. } => unreachable!(),
        }
    }

    /// Tries to insert a key-value pair into the map, and returns
    /// a reference to the value that was inserted.
    ///
    /// See [`HashMap::try_insert`] for details.
    #[inline]
    pub fn try_insert(&self, key: K, value: V) -> Result<&V, OccupiedError<'_, V>> {
        match self.root().insert(key, value, false, &self.guard) {
            InsertResult::Inserted(value) => Ok(value),
            InsertResult::Error {
                current,
                not_inserted,
            } => Err(OccupiedError {
                current,
                not_inserted,
            }),
            InsertResult::Replaced(_) => unreachable!(),
        }
    }

    /// Returns a reference to the value corresponding to the key, or inserts a default value.
    ///
    /// See [`HashMap::get_or_insert`] for details.
    #[inline]
    pub fn get_or_insert(&self, key: K, value: V) -> &V {
        // Note that we use `try_insert` instead of `compute` or `get_or_insert_with` here, as it
        // allows us to avoid the closure indirection.
        match self.try_insert(key, value) {
            Ok(inserted) => inserted,
            Err(OccupiedError { current, .. }) => current,
        }
    }

    /// Returns a reference to the value corresponding to the key, or inserts a default value
    /// computed from a closure.
    ///
    /// See [`HashMap::get_or_insert_with`] for details.
    #[inline]
    pub fn get_or_insert_with<F>(&self, key: K, f: F) -> &V
    where
        F: FnOnce() -> V,
    {
        self.root().get_or_insert_with(key, f, &self.guard)
    }

    /// Updates an existing entry atomically.
    ///
    /// See [`HashMap::update`] for details.
    #[inline]
    pub fn update<F>(&self, key: K, update: F) -> Option<&V>
    where
        F: Fn(&V) -> V,
    {
        self.root().update(key, update, &self.guard)
    }

    /// Updates an existing entry or inserts a default value.
    ///
    /// See [`HashMap::update_or_insert`] for details.
    #[inline]
    pub fn update_or_insert<F>(&self, key: K, update: F, value: V) -> &V
    where
        F: Fn(&V) -> V,
    {
        self.update_or_insert_with(key, update, || value)
    }

    /// Updates an existing entry or inserts a default value computed from a closure.
    ///
    /// See [`HashMap::update_or_insert_with`] for details.
    #[inline]
    pub fn update_or_insert_with<U, F>(&self, key: K, update: U, f: F) -> &V
    where
        F: FnOnce() -> V,
        U: Fn(&V) -> V,
    {
        self.root()
            .update_or_insert_with(key, update, f, &self.guard)
    }

    // Updates an entry with a compare-and-swap (CAS) function.
    //
    /// See [`HashMap::compute`] for details.
    #[inline]
    pub fn compute<'g, F, T>(&'g self, key: K, compute: F) -> Compute<'g, K, V, T>
    where
        F: FnMut(Option<(&'g K, &'g V)>) -> Operation<V, T>,
    {
        self.root().compute(key, compute, &self.guard)
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    ///
    /// See [`HashMap::remove`] for details.
    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self.root().remove(key, &self.guard) {
            Some((_, value)) => Some(value),
            None => None,
        }
    }

    /// Removes a key from the map, returning the stored key and value if the
    /// key was previously in the map.
    ///
    /// See [`HashMap::remove_entry`] for details.
    #[inline]
    pub fn remove_entry<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.root().remove(key, &self.guard)
    }

    /// Clears the map, removing all key-value pairs.
    ///
    /// See [`HashMap::clear`] for details.
    #[inline]
    pub fn clear(&self) {
        self.root().clear(&self.guard)
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// See [`HashMap::retain`] for details.
    #[inline]
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.root().retain(f, &self.guard)
    }

    /// Tries to reserve capacity for `additional` more elements to be inserted
    /// in the map.
    ///
    /// See [`HashMap::reserve`] for details.
    #[inline]
    pub fn reserve(&self, additional: usize) {
        self.root().reserve(additional, &self.guard)
    }

    /// An iterator visiting all key-value pairs in arbitrary order.
    /// The iterator element type is `(&K, &V)`.
    ///
    /// See [`HashMap::iter`] for details.
    #[inline]
    pub fn iter(&self) -> Iter<'_, K, V, G> {
        Iter {
            raw: self.root().iter(&self.guard),
        }
    }

    /// An iterator visiting all keys in arbitrary order.
    /// The iterator element type is `&K`.
    ///
    /// See [`HashMap::keys`] for details.
    #[inline]
    pub fn keys(&self) -> Keys<'_, K, V, G> {
        Keys { iter: self.iter() }
    }

    /// An iterator visiting all values in arbitrary order.
    /// The iterator element type is `&V`.
    ///
    /// See [`HashMap::values`] for details.
    #[inline]
    pub fn values(&self) -> Values<'_, K, V, G> {
        Values { iter: self.iter() }
    }

    #[inline]
    fn root(&self) -> raw::HashMapRef<'_, K, V, S> {
        // Safety: A `HashMapRef` can only be created through `HashMap::pin` or
        // `HashMap::pin_owned`, so we know the guard belongs to our collector.
        unsafe { self.map.raw.root_unchecked(&self.guard) }
    }
}

impl<K, V, S, G> fmt::Debug for HashMapRef<'_, K, V, S, G>
where
    K: Hash + Eq + fmt::Debug,
    V: fmt::Debug,
    S: BuildHasher,
    G: Guard,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<'a, K, V, S, G> IntoIterator for &'a HashMapRef<'_, K, V, S, G>
where
    K: Hash + Eq,
    S: BuildHasher,
    G: Guard,
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V, G>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over a map's entries.
///
/// This struct is created by the [`iter`](HashMap::iter) method on [`HashMap`]. See its documentation for details.
pub struct Iter<'g, K, V, G> {
    raw: raw::Iter<'g, K, V, G>,
}

impl<'g, K: 'g, V: 'g, G> Iterator for Iter<'g, K, V, G>
where
    G: Guard,
{
    type Item = (&'g K, &'g V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.raw.next()
    }
}

impl<K, V, G> fmt::Debug for Iter<'_, K, V, G>
where
    K: fmt::Debug,
    V: fmt::Debug,
    G: Guard,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(Iter {
                raw: self.raw.clone(),
            })
            .finish()
    }
}

/// An iterator over a map's keys.
///
/// This struct is created by the [`keys`](HashMap::keys) method on [`HashMap`]. See its documentation for details.
pub struct Keys<'g, K, V, G> {
    iter: Iter<'g, K, V, G>,
}

impl<'g, K: 'g, V: 'g, G> Iterator for Keys<'g, K, V, G>
where
    G: Guard,
{
    type Item = &'g K;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (key, _) = self.iter.next()?;
        Some(key)
    }
}

impl<K, V, G> fmt::Debug for Keys<'_, K, V, G>
where
    K: fmt::Debug,
    V: fmt::Debug,
    G: Guard,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Keys").field(&self.iter).finish()
    }
}

/// An iterator over a map's values.
///
/// This struct is created by the [`values`](HashMap::values) method on [`HashMap`]. See its documentation for details.
pub struct Values<'g, K, V, G> {
    iter: Iter<'g, K, V, G>,
}

impl<'g, K: 'g, V: 'g, G> Iterator for Values<'g, K, V, G>
where
    G: Guard,
{
    type Item = &'g V;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (_, value) = self.iter.next()?;
        Some(value)
    }
}

impl<K, V, G> fmt::Debug for Values<'_, K, V, G>
where
    K: fmt::Debug,
    V: fmt::Debug,
    G: Guard,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Values").field(&self.iter).finish()
    }
}
