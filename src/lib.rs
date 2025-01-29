#![warn(
    unsafe_op_in_unsafe_fn,
    clippy::missing_safety_doc,
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks
)]

pub mod iter;
pub mod iter_set;
mod lock;
pub mod mapref;
mod read_only;
#[cfg(feature = "serde")]
mod serde;
mod set;
pub mod setref;
pub mod try_result;
mod util;

#[cfg(feature = "rayon")]
pub mod rayon {
    pub mod map;
    pub mod read_only;
    pub mod set;
}

#[cfg(not(feature = "raw-api"))]
use crate::lock::RwLock;

#[cfg(feature = "raw-api")]
pub use crate::lock::{RawRwLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use core::fmt;
use core::hash::{BuildHasher, Hash, Hasher};
use core::iter::FromIterator;
use core::ops::{BitAnd, BitOr, Shl, Shr, Sub};
use crossbeam_utils::CachePadded;
use hashbrown::{hash_table, Equivalent};
use iter::{Iter, IterMut, OwningIter};
use lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
pub use mapref::entry::{Entry, OccupiedEntry, VacantEntry};
use mapref::entrymut::{EntryMut, OccupiedEntryMut, VacantEntryMut};
use mapref::multiple::RefMulti;
use mapref::one::{Ref, RefMut};
pub use read_only::ReadOnlyView;
use replace_with::replace_with_or_abort;
pub use set::ClashSet;
use std::collections::hash_map::RandomState;
use std::convert::Infallible;
use std::sync::OnceLock;
use try_result::TryResult;

pub(crate) type HashMap<K, V> = hash_table::HashTable<(K, V)>;
pub(crate) type Shard<K, V> = CachePadded<RwLock<HashMap<K, V>>>;

// Temporary reimplementation of [`std::collections::TryReserveError`]
// util [`std::collections::TryReserveError`] stabilises.
// We cannot easily create `std::collections` error type from `hashbrown` error type
// without access to `TryReserveError::kind` method.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TryReserveError {}

#[no_mangle]
fn default_shard_amount() -> usize {
    static DEFAULT_SHARD_AMOUNT: OnceLock<usize> = OnceLock::new();
    *DEFAULT_SHARD_AMOUNT.get_or_init(|| {
        (std::thread::available_parallelism().map_or(1, usize::from) * 4).next_power_of_two()
    })
}

/// ClashMap is an implementation of a concurrent associative array/hashmap in Rust.
///
/// ClashMap tries to implement an easy to use API similar to `std::collections::HashMap`
/// with some slight changes to handle concurrency.
///
/// ClashMap tries to be very simple to use and to be a direct replacement for `RwLock<HashMap<K, V>>`.
/// To accomplish this, all methods take `&self` instead of modifying methods taking `&mut self`.
/// This allows you to put a ClashMap in an `Arc<T>` and share it between threads while being able to modify it.
///
/// Documentation mentioning locking behaviour acts in the reference frame of the calling thread.
/// This means that it is safe to ignore it across multiple threads.
pub struct ClashMap<K, V, S = RandomState> {
    shift: usize,
    shards: Box<[Shard<K, V>]>,
    hasher: S,
}

impl<K: Eq + Hash + Clone, V: Clone, S: Clone> Clone for ClashMap<K, V, S> {
    fn clone(&self) -> Self {
        let mut inner_shards = Vec::new();

        for shard in self.shards.iter() {
            let shard = shard.read();

            inner_shards.push(CachePadded::new(RwLock::new((*shard).clone())));
        }

        Self {
            shift: self.shift,
            shards: inner_shards.into_boxed_slice(),
            hasher: self.hasher.clone(),
        }
    }
}

impl<K, V, S> Default for ClashMap<K, V, S>
where
    K: Eq + Hash,
    S: Default + BuildHasher,
{
    fn default() -> Self {
        Self::with_hasher(Default::default())
    }
}

impl<K: Eq + Hash, V> ClashMap<K, V, RandomState> {
    /// Creates a new ClashMap with a capacity of 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let reviews = ClashMap::new();
    /// reviews.insert("Veloren", "What a fantastic game!");
    /// ```
    pub fn new() -> Self {
        ClashMap::with_hasher(RandomState::default())
    }

    /// Creates a new ClashMap with a specified starting capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let mappings = ClashMap::with_capacity(2);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        ClashMap::with_capacity_and_hasher(capacity, RandomState::default())
    }

    /// Creates a new ClashMap with a specified shard amount
    ///
    /// shard_amount should greater than 0 and be a power of two.
    /// If a shard_amount which is not a power of two is provided, the function will panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let mappings = ClashMap::with_shard_amount(32);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    pub fn with_shard_amount(shard_amount: usize) -> Self {
        Self::with_capacity_and_hasher_and_shard_amount(0, RandomState::default(), shard_amount)
    }

    /// Creates a new ClashMap with a specified capacity and shard amount.
    ///
    /// shard_amount should greater than 0 and be a power of two.
    /// If a shard_amount which is not a power of two is provided, the function will panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let mappings = ClashMap::with_capacity_and_shard_amount(32, 32);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    pub fn with_capacity_and_shard_amount(capacity: usize, shard_amount: usize) -> Self {
        Self::with_capacity_and_hasher_and_shard_amount(
            capacity,
            RandomState::default(),
            shard_amount,
        )
    }
}

#[cfg(feature = "raw-api")]
impl<K: Eq + Hash, V, S: BuildHasher> ClashMap<K, V, S> {
    /// Allows you to peek at the inner shards that store your data.
    /// You should probably not use this unless you know what you are doing.
    ///
    /// Requires the `raw-api` feature to be enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let map = ClashMap::<(), ()>::new();
    /// println!("Amount of shards: {}", map.shards().len());
    /// ```
    pub fn shards(&self) -> &[CachePadded<RwLock<HashMap<K, V>>>] {
        &self.shards
    }

    /// Provides mutable access to the inner shards that store your data.
    /// You should probably not use this unless you know what you are doing.
    ///
    /// Requires the `raw-api` feature to be enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use std::hash::{Hash, Hasher, BuildHasher};
    ///
    /// let mut map = ClashMap::<i32, &'static str>::new();
    /// let shard_ind = map.determine_map(&42);
    /// let mut factory = map.hasher().clone();
    /// let hasher = |tuple: &(i32, &'static str)| {
    ///     let mut hasher = factory.build_hasher();
    ///     tuple.0.hash(&mut hasher);
    ///     hasher.finish()
    /// };
    /// let data = (42, "forty two");
    /// let hash = hasher(&data);
    /// map.shards_mut()[shard_ind].get_mut().insert_unique(hash, data, hasher);
    /// assert_eq!(*map.get(&42).unwrap(), "forty two");
    /// ```
    pub fn shards_mut(&mut self) -> &mut [Shard<K, V>] {
        &mut self.shards
    }

    /// Consumes this `ClashMap` and returns the inner shards.
    /// You should probably not use this unless you know what you are doing.
    ///
    /// Requires the `raw-api` feature to be enabled.
    ///
    /// See [`ClashMap::shards()`] and [`ClashMap::shards_mut()`] for more information.
    pub fn into_shards(self) -> Box<[Shard<K, V>]> {
        self.shards
    }

    /// Finds which shard a certain key is stored in.
    /// You should probably not use this unless you know what you are doing.
    /// Note that shard selection is dependent on the default or provided HashBuilder.
    ///
    /// Requires the `raw-api` feature to be enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let map = ClashMap::new();
    /// map.insert("coca-cola", 1.4);
    /// println!("coca-cola is stored in shard: {}", map.determine_map("coca-cola"));
    /// ```
    pub fn determine_map<Q>(&self, key: &Q) -> usize
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_usize(&key);
        self._determine_shard(hash)
    }

    /// Finds which shard a certain hash is stored in.
    ///
    /// Requires the `raw-api` feature to be enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let map: ClashMap<i32, i32> = ClashMap::new();
    /// let key = "key";
    /// let hash = map.hash_usize(&key);
    /// println!("hash is stored in shard: {}", map.determine_shard(hash));
    /// ```
    pub fn determine_shard(&self, hash: usize) -> usize {
        self._determine_shard(hash)
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> ClashMap<K, V, S> {
    /// Wraps this `ClashMap` into a read-only view. This view allows to obtain raw references to the stored values.
    pub fn into_read_only(self) -> ReadOnlyView<K, V, S> {
        ReadOnlyView::new(self)
    }

    /// Creates a new ClashMap with a capacity of 0 and the provided hasher.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let reviews = ClashMap::with_hasher(s);
    /// reviews.insert("Veloren", "What a fantastic game!");
    /// ```
    pub fn with_hasher(hasher: S) -> Self {
        Self::with_capacity_and_hasher(0, hasher)
    }

    /// Creates a new ClashMap with a specified starting capacity and hasher.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let mappings = ClashMap::with_capacity_and_hasher(2, s);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    pub fn with_capacity_and_hasher(capacity: usize, hasher: S) -> Self {
        Self::with_capacity_and_hasher_and_shard_amount(capacity, hasher, default_shard_amount())
    }

    /// Creates a new ClashMap with a specified hasher and shard amount
    ///
    /// shard_amount should be greater than 0 and a power of two.
    /// If a shard_amount which is not a power of two is provided, the function will panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let mappings = ClashMap::with_hasher_and_shard_amount(s, 32);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    pub fn with_hasher_and_shard_amount(hasher: S, shard_amount: usize) -> Self {
        Self::with_capacity_and_hasher_and_shard_amount(0, hasher, shard_amount)
    }

    /// Creates a new ClashMap with a specified starting capacity, hasher and shard_amount.
    ///
    /// shard_amount should greater than 0 and be a power of two.
    /// If a shard_amount which is not a power of two is provided, the function will panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let mappings = ClashMap::with_capacity_and_hasher_and_shard_amount(2, s, 32);
    /// mappings.insert(2, 4);
    /// mappings.insert(8, 16);
    /// ```
    pub fn with_capacity_and_hasher_and_shard_amount(
        mut capacity: usize,
        hasher: S,
        shard_amount: usize,
    ) -> Self {
        assert!(shard_amount > 1);
        assert!(shard_amount.is_power_of_two());

        let shift = (usize::BITS - shard_amount.trailing_zeros()) as usize;

        if capacity != 0 {
            capacity = (capacity + (shard_amount - 1)) & !(shard_amount - 1);
        }

        let cps = capacity / shard_amount;

        let shards = (0..shard_amount)
            .map(|_| CachePadded::new(RwLock::new(HashMap::with_capacity(cps))))
            .collect();

        Self {
            shift,
            shards,
            hasher,
        }
    }

    /// Hash a given item to produce a usize.
    /// Uses the provided or default HashBuilder.
    pub fn hash_usize<T: Hash>(&self, item: &T) -> usize {
        self.hash_u64(item) as usize
    }

    fn hash_u64<T: Hash>(&self, item: &T) -> u64 {
        let mut hasher = self.hasher.build_hasher();

        item.hash(&mut hasher);

        hasher.finish()
    }

    #[inline(always)]
    pub(crate) fn _determine_shard(&self, hash: usize) -> usize {
        // Leave the high 7 bits for the HashBrown SIMD tag.
        let idx = (hash << 7) >> self.shift;

        // hint to llvm that the panic bounds check can be removed
        if idx >= self.shards.len() {
            if cfg!(debug_assertions) {
                unreachable!("invalid shard index")
            } else {
                // SAFETY: shards is always a power of two,
                // and shift is calculated such that the resulting idx is always
                // less than the shards length
                unsafe {
                    std::hint::unreachable_unchecked();
                }
            }
        }

        idx
    }

    /// Returns a reference to the map's [`BuildHasher`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use clashmap::ClashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let hasher = RandomState::new();
    /// let map: ClashMap<i32, i32> = ClashMap::new();
    /// let hasher: &RandomState = map.hasher();
    /// ```
    ///
    /// [`BuildHasher`]: https://doc.rust-lang.org/std/hash/trait.BuildHasher.html
    pub fn hasher(&self) -> &S {
        &self.hasher
    }

    /// Inserts a key and a value into the map. Returns the old value associated with the key if there was one.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let map = ClashMap::new();
    /// map.insert("I am the key!", "And I am the value!");
    /// ```
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        match self.entry(key) {
            Entry::Occupied(mut o) => Some(o.insert(value)),
            Entry::Vacant(v) => {
                v.insert(value);
                None
            }
        }
    }

    /// Inserts a key and a value into the map. Returns the old value associated with the key if there was one.
    //
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let mut map = ClashMap::new();
    /// map.insert_mut("I am the key!", "And I am the value!");
    /// ```
    pub fn insert_mut(&mut self, key: K, value: V) -> Option<V> {
        match self.entry_mut(key) {
            EntryMut::Occupied(mut o) => Some(o.insert(value)),
            EntryMut::Vacant(v) => {
                v.insert(value);
                None
            }
        }
    }

    /// Removes an entry from the map, returning the key and value if they existed in the map.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let soccer_team = ClashMap::new();
    /// soccer_team.insert("Jack", "Goalie");
    /// assert_eq!(soccer_team.remove("Jack").unwrap().1, "Goalie");
    /// ```
    pub fn remove<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let mut shard = self.shards[idx].write();

        if let Ok(entry) = shard.find_entry(hash, |(k, _v)| key.equivalent(k)) {
            let ((k, v), _) = entry.remove();
            Some((k, v))
        } else {
            None
        }
    }

    /// Removes an entry from the map, returning the key and value
    /// if the entry existed and the provided conditional function returned true.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let soccer_team = ClashMap::new();
    /// soccer_team.insert("Sam", "Forward");
    /// soccer_team.remove_if("Sam", |_, position| position == &"Goalie");
    /// assert!(soccer_team.contains_key("Sam"));
    /// ```
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let soccer_team = ClashMap::new();
    /// soccer_team.insert("Sam", "Forward");
    /// soccer_team.remove_if("Sam", |_, position| position == &"Forward");
    /// assert!(!soccer_team.contains_key("Sam"));
    /// ```
    pub fn remove_if<Q>(&self, key: &Q, f: impl FnOnce(&K, &V) -> bool) -> Option<(K, V)>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let mut shard = self.shards[idx].write();

        if let Ok(entry) = shard.find_entry(hash, |(k, _v)| key.equivalent(k)) {
            let (k, v) = entry.get();
            if f(k, v) {
                let ((k, v), _) = entry.remove();
                Some((k, v))
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn remove_if_mut<Q>(&self, key: &Q, f: impl FnOnce(&K, &mut V) -> bool) -> Option<(K, V)>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let mut shard = self.shards[idx].write();

        if let Ok(mut entry) = shard.find_entry(hash, |(k, _v)| key.equivalent(k)) {
            let (k, v) = entry.get_mut();
            if f(k, v) {
                let ((k, v), _) = entry.remove();
                Some((k, v))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Creates an iterator over a ClashMap yielding immutable references.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let words = ClashMap::new();
    /// words.insert("hello", "world");
    /// assert_eq!(words.iter().count(), 1);
    /// ```
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter::new(self)
    }

    fn for_each(&self, mut f: impl FnMut(&(K, V))) {
        self.fold((), |(), kv| f(kv))
    }

    fn fold<R>(&self, r: R, mut f: impl FnMut(R, &(K, V)) -> R) -> R {
        match self.try_fold::<R, Infallible>(r, |r, kv| Ok(f(r, kv))) {
            Ok(r) => r,
            Err(x) => match x {},
        }
    }

    #[allow(dead_code)]
    fn try_for_each<E>(&self, mut f: impl FnMut(&(K, V)) -> Result<(), E>) -> Result<(), E> {
        self.try_fold((), |(), kv| f(kv))
    }

    fn try_fold<R, E>(
        &self,
        mut r: R,
        mut f: impl FnMut(R, &(K, V)) -> Result<R, E>,
    ) -> Result<R, E> {
        for shard in &self.shards {
            let shard = shard.read();
            r = shard.iter().try_fold(r, &mut f)?;
        }
        Ok(r)
    }

    /// Iterator over a ClashMap yielding mutable references.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let map = ClashMap::new();
    /// map.insert("Johnny", 21);
    /// map.iter_mut().for_each(|mut r| *r += 1);
    /// assert_eq!(*map.get("Johnny").unwrap(), 22);
    /// ```
    pub fn iter_mut(&self) -> IterMut<'_, K, V> {
        IterMut::new(self)
    }

    /// Get an immutable reference to an entry in the map
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let youtubers = ClashMap::new();
    /// youtubers.insert("Bosnian Bill", 457000);
    /// assert_eq!(*youtubers.get("Bosnian Bill").unwrap(), 457000);
    /// ```
    pub fn get<Q>(&self, key: &Q) -> Option<Ref<'_, K, V>>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].read();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };

        if let Some(entry) = shard.find(hash, |(k, _v)| key.equivalent(k)) {
            let (k, v) = entry;
            Some(Ref::new(guard, k, v))
        } else {
            None
        }
    }

    /// Get a mutable reference to an entry in the map
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let class = ClashMap::new();
    /// class.insert("Albin", 15);
    /// *class.get_mut("Albin").unwrap() -= 1;
    /// assert_eq!(*class.get("Albin").unwrap(), 14);
    /// ```
    pub fn get_mut<Q>(&self, key: &Q) -> Option<RefMut<'_, K, V>>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].write();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `RefMut`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        if let Ok(entry) = shard.find_entry(hash, |(k, _v)| key.equivalent(k)) {
            let (k, v) = entry.into_mut();
            Some(RefMut::new(guard, k, v))
        } else {
            None
        }
    }

    /// Get an immutable reference to an entry in the map, if the shard is not locked.
    /// If the shard is locked, the function will return [TryResult::Locked].
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use clashmap::try_result::TryResult;
    ///
    /// let map = ClashMap::new();
    /// map.insert("Johnny", 21);
    ///
    /// assert_eq!(*map.try_get("Johnny").unwrap(), 21);
    ///
    /// let _result1_locking = map.get_mut("Johnny");
    ///
    /// let result2 = map.try_get("Johnny");
    /// assert!(result2.is_locked());
    /// ```
    pub fn try_get<Q>(&self, key: &Q) -> TryResult<Ref<'_, K, V>>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let shard = match self.shards[idx].try_read() {
            Some(shard) => shard,
            None => return TryResult::Locked,
        };

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };

        if let Some(entry) = shard.find(hash, |(k, _v)| key.equivalent(k)) {
            let (k, v) = entry;
            TryResult::Present(Ref::new(guard, k, v))
        } else {
            TryResult::Absent
        }
    }

    /// Get a mutable reference to an entry in the map, if the shard is not locked.
    /// If the shard is locked, the function will return [TryResult::Locked].
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use clashmap::try_result::TryResult;
    ///
    /// let map = ClashMap::new();
    /// map.insert("Johnny", 21);
    ///
    /// *map.try_get_mut("Johnny").unwrap() += 1;
    /// assert_eq!(*map.get("Johnny").unwrap(), 22);
    ///
    /// let _result1_locking = map.get("Johnny");
    ///
    /// let result2 = map.try_get_mut("Johnny");
    /// assert!(result2.is_locked());
    /// ```
    pub fn try_get_mut<Q>(&self, key: &Q) -> TryResult<RefMut<'_, K, V>>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let shard = match self.shards[idx].try_write() {
            Some(shard) => shard,
            None => return TryResult::Locked,
        };

        // SAFETY: The data will not outlive the guard, since we pass the guard to `RefMut`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        if let Ok(entry) = shard.find_entry(hash, |(k, _v)| key.equivalent(k)) {
            let (k, v) = entry.into_mut();
            TryResult::Present(RefMut::new(guard, k, v))
        } else {
            TryResult::Absent
        }
    }

    /// Remove excess capacity to reduce memory usage.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    /// use clashmap::try_result::TryResult;
    ///
    /// let map = ClashMap::new();
    /// map.insert("Johnny", 21);
    /// assert!(map.capacity() > 0);
    /// map.remove("Johnny");
    /// map.shrink_to_fit();
    /// assert_eq!(map.capacity(), 0);
    /// ```
    pub fn shrink_to_fit(&self) {
        self.shards.iter().for_each(|s| {
            let mut shard = s.write();
            let size = shard.len();
            shard.shrink_to(size, |(k, _v)| {
                let mut hasher = self.hasher.build_hasher();
                k.hash(&mut hasher);
                hasher.finish()
            })
        })
    }

    /// Retain elements that whose predicates return true
    /// and discard elements whose predicates return false.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let people = ClashMap::new();
    /// people.insert("Albin", 15);
    /// people.insert("Jones", 22);
    /// people.insert("Charlie", 27);
    /// people.retain(|_, v| *v > 20);
    /// assert_eq!(people.len(), 2);
    /// ```
    pub fn retain(&self, mut f: impl FnMut(&K, &mut V) -> bool) {
        self.shards.iter().for_each(|s| {
            s.write().retain(|(k, v)| f(k, v));
        })
    }

    /// Fetches the total number of key-value pairs stored in the map.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let people = ClashMap::new();
    /// people.insert("Albin", 15);
    /// people.insert("Jones", 22);
    /// people.insert("Charlie", 27);
    /// assert_eq!(people.len(), 3);
    /// ```
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().len()).sum()
    }

    /// Checks if the map is empty or not.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let map = ClashMap::<(), ()>::new();
    /// assert!(map.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes all key-value pairs in the map.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let stats = ClashMap::new();
    /// stats.insert("Goals", 4);
    /// assert!(!stats.is_empty());
    /// stats.clear();
    /// assert!(stats.is_empty());
    /// ```
    pub fn clear(&self) {
        self.retain(|_, _| false)
    }

    /// Returns how many key-value pairs the map can store without reallocating.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    pub fn capacity(&self) -> usize {
        self.shards.iter().map(|s| s.read().capacity()).sum()
    }

    /// Modify a specific value according to a function.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let stats = ClashMap::new();
    /// stats.insert("Goals", 4);
    /// stats.alter("Goals", |_, v| v * 2);
    /// assert_eq!(*stats.get("Goals").unwrap(), 8);
    /// ```
    ///
    /// # Panics
    ///
    /// If the given closure panics, then `alter` will abort the process
    pub fn alter<Q>(&self, key: &Q, f: impl FnOnce(&K, V) -> V)
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        if let Some(mut r) = self.get_mut(key) {
            let (k, v) = r.pair_mut();
            replace_with_or_abort(v, |v| f(k, v))
        }
    }

    /// Modify every value in the map according to a function.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let stats = ClashMap::new();
    /// stats.insert("Wins", 4);
    /// stats.insert("Losses", 2);
    /// stats.alter_all(|_, v| v + 1);
    /// assert_eq!(*stats.get("Wins").unwrap(), 5);
    /// assert_eq!(*stats.get("Losses").unwrap(), 3);
    /// ```
    ///
    /// # Panics
    ///
    /// If the given closure panics, then `alter_all` will abort the process
    pub fn alter_all(&self, mut f: impl FnMut(&K, V) -> V) {
        self.iter_mut().for_each(|mut m| {
            let (k, v) = m.pair_mut();
            replace_with_or_abort(v, |v| f(k, v))
        })
    }

    /// Scoped access into an item of the map according to a function.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let warehouse = ClashMap::new();
    /// warehouse.insert(4267, ("Banana", 100));
    /// warehouse.insert(2359, ("Pear", 120));
    /// let fruit = warehouse.view(&4267, |_k, v| *v);
    /// assert_eq!(fruit, Some(("Banana", 100)));
    /// ```
    ///
    /// # Panics
    ///
    /// If the given closure panics, then `view` will abort the process
    pub fn view<Q, R>(&self, key: &Q, f: impl FnOnce(&K, &V) -> R) -> Option<R>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.get(key).map(|r| {
            let (k, v) = r.pair();
            f(k, v)
        })
    }

    /// Checks if the map contains a specific key.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let team_sizes = ClashMap::new();
    /// team_sizes.insert("Dakota Cherries", 23);
    /// assert!(team_sizes.contains_key("Dakota Cherries"));
    /// ```
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.get(key).is_some()
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap`.
    pub fn entry_mut(&mut self, key: K) -> EntryMut<'_, K, V> {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].get_mut();

        match shard.entry(
            hash,
            |(k, _v)| k == &key,
            |(k, _v)| {
                let mut hasher = self.hasher.build_hasher();
                k.hash(&mut hasher);
                hasher.finish()
            },
        ) {
            hash_table::Entry::Occupied(occupied_entry) => {
                EntryMut::Occupied(OccupiedEntryMut::new(key, occupied_entry))
            }
            hash_table::Entry::Vacant(vacant_entry) => {
                EntryMut::Vacant(VacantEntryMut::new(key, vacant_entry))
            }
        }
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap`.
    /// See the documentation on `clashmap::mapref::entry` for more details.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn entry(&self, key: K) -> Entry<'_, K, V> {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].write();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Entry`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        match shard.entry(
            hash,
            |(k, _v)| k == &key,
            |(k, _v)| {
                let mut hasher = self.hasher.build_hasher();
                k.hash(&mut hasher);
                hasher.finish()
            },
        ) {
            hash_table::Entry::Occupied(occupied_entry) => {
                Entry::Occupied(OccupiedEntry::new(guard, key, occupied_entry))
            }
            hash_table::Entry::Vacant(vacant_entry) => {
                Entry::Vacant(VacantEntry::new(guard, key, vacant_entry))
            }
        }
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap`.
    /// See the documentation on `clashmap::mapref::entry` for more details.
    ///
    /// Returns None if the shard is currently locked.
    pub fn try_entry(&self, key: K) -> Option<Entry<'_, K, V>> {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].try_write()?;

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Entry`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        match shard.entry(
            hash,
            |(k, _v)| k == &key,
            |(k, _v)| {
                let mut hasher = self.hasher.build_hasher();
                k.hash(&mut hasher);
                hasher.finish()
            },
        ) {
            hash_table::Entry::Occupied(occupied_entry) => Some(Entry::Occupied(
                OccupiedEntry::new(guard, key, occupied_entry),
            )),
            hash_table::Entry::Vacant(vacant_entry) => {
                Some(Entry::Vacant(VacantEntry::new(guard, key, vacant_entry)))
            }
        }
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap::try_reserve`.
    /// Tries to reserve capacity for at least `shard * additional`
    /// and may reserve more space to avoid frequent reallocations.
    ///
    /// # Errors
    ///
    /// If the capacity overflows, or the allocator reports a failure, then an error is returned.
    // TODO: return std::collections::TryReserveError once std::collections::TryReserveErrorKind stabilises.
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        for shard in self.shards.iter() {
            shard
                .write()
                .try_reserve(additional, |(k, _v)| {
                    let mut hasher = self.hasher.build_hasher();
                    k.hash(&mut hasher);
                    hasher.finish()
                })
                .map_err(|_| TryReserveError {})?;
        }
        Ok(())
    }
}

impl<K: Eq + Hash + fmt::Debug, V: fmt::Debug, S: BuildHasher> fmt::Debug for ClashMap<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut pmap = f.debug_map();
        self.for_each(|(k, v)| {
            pmap.entry(k, v);
        });
        pmap.finish()
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: BuildHasher> Shl<(K, V)> for &'a ClashMap<K, V, S> {
    type Output = Option<V>;

    fn shl(self, pair: (K, V)) -> Self::Output {
        self.insert(pair.0, pair.1)
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: BuildHasher, Q> Shr<&Q> for &'a ClashMap<K, V, S>
where
    Q: Hash + Equivalent<K> + ?Sized,
{
    type Output = Ref<'a, K, V>;

    fn shr(self, key: &Q) -> Self::Output {
        self.get(key).unwrap()
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: BuildHasher, Q> BitOr<&Q> for &'a ClashMap<K, V, S>
where
    Q: Hash + Equivalent<K> + ?Sized,
{
    type Output = RefMut<'a, K, V>;

    fn bitor(self, key: &Q) -> Self::Output {
        self.get_mut(key).unwrap()
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: BuildHasher, Q> Sub<&Q> for &'a ClashMap<K, V, S>
where
    Q: Hash + Equivalent<K> + ?Sized,
{
    type Output = Option<(K, V)>;

    fn sub(self, key: &Q) -> Self::Output {
        self.remove(key)
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: BuildHasher, Q> BitAnd<&Q> for &'a ClashMap<K, V, S>
where
    Q: Hash + Equivalent<K> + ?Sized,
{
    type Output = bool;

    fn bitand(self, key: &Q) -> Self::Output {
        self.contains_key(key)
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> IntoIterator for ClashMap<K, V, S> {
    type Item = (K, V);

    type IntoIter = OwningIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        OwningIter::new(self)
    }
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> IntoIterator for &'a ClashMap<K, V, S> {
    type Item = RefMulti<'a, K, V>;

    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> Extend<(K, V)> for ClashMap<K, V, S> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, intoiter: I) {
        for pair in intoiter.into_iter() {
            self.insert_mut(pair.0, pair.1);
        }
    }
}

impl<K: Eq + Hash, V, S: BuildHasher + Default> FromIterator<(K, V)> for ClashMap<K, V, S> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(intoiter: I) -> Self {
        let mut map = ClashMap::default();

        map.extend(intoiter);

        map
    }
}

#[cfg(feature = "typesize")]
impl<K, V, S> typesize::TypeSize for ClashMap<K, V, S>
where
    K: typesize::TypeSize + Eq + Hash,
    V: typesize::TypeSize,
    S: typesize::TypeSize + BuildHasher,
{
    fn extra_size(&self) -> usize {
        let shards_extra_size: usize = self
            .shards
            .iter()
            .map(|shard_lock| {
                let shard = shard_lock.read();

                let hashtable_size = shard.allocation_size();

                let entry_size_iter = shard.iter().map(|entry| {
                    let (key, value) = entry;
                    key.extra_size() + value.extra_size()
                });

                core::mem::size_of::<CachePadded<RwLock<HashMap<K, V>>>>()
                    + hashtable_size
                    + entry_size_iter.sum::<usize>()
            })
            .sum();

        self.hasher.extra_size() + shards_extra_size
    }

    typesize::if_typesize_details! {
        fn get_collection_item_count(&self) -> Option<usize> {
            Some(self.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ClashMap;
    use std::collections::hash_map::RandomState;

    #[test]
    fn test_basic() {
        let dm = ClashMap::new();

        dm.insert(0, 0);

        assert_eq!(dm.get(&0).unwrap().value(), &0);
    }

    #[test]
    fn test_default() {
        let dm: ClashMap<u32, u32> = ClashMap::default();

        dm.insert(0, 0);

        assert_eq!(dm.get(&0).unwrap().value(), &0);
    }

    #[test]
    fn test_multiple_hashes() {
        let dm: ClashMap<u32, u32> = ClashMap::default();

        for i in 0..100 {
            dm.insert(0, i);

            dm.insert(i, i);
        }

        for i in 1..100 {
            let r = dm.get(&i).unwrap();

            assert_eq!(i, *r.value());

            assert_eq!(i, *r.key());
        }

        let r = dm.get(&0).unwrap();

        assert_eq!(99, *r.value());
    }

    #[test]
    fn test_more_complex_values() {
        #[derive(Hash, PartialEq, Debug, Clone)]

        struct T0 {
            s: String,
            u: u8,
        }

        let dm = ClashMap::new();

        let range = 0..10;

        for i in range {
            let t = T0 {
                s: i.to_string(),
                u: i as u8,
            };

            dm.insert(i, t.clone());

            assert_eq!(&t, dm.get(&i).unwrap().value());
        }
    }

    #[test]
    fn test_different_hashers_randomstate() {
        let dm_hm_default: ClashMap<u32, u32, RandomState> =
            ClashMap::with_hasher(RandomState::new());

        for i in 0..10 {
            dm_hm_default.insert(i, i);

            assert_eq!(i, *dm_hm_default.get(&i).unwrap().value());
        }
    }

    #[test]
    fn test_map_view() {
        let dm = ClashMap::new();

        let vegetables: [String; 4] = [
            "Salad".to_string(),
            "Beans".to_string(),
            "Potato".to_string(),
            "Tomato".to_string(),
        ];

        // Give it some values
        dm.insert(0, "Banana".to_string());
        dm.insert(4, "Pear".to_string());
        dm.insert(9, "Potato".to_string());
        dm.insert(12, "Chicken".to_string());

        let potato_vegetableness = dm.view(&9, |_, v| vegetables.contains(v));
        assert_eq!(potato_vegetableness, Some(true));

        let chicken_vegetableness = dm.view(&12, |_, v| vegetables.contains(v));
        assert_eq!(chicken_vegetableness, Some(false));

        let not_in_map = dm.view(&30, |_k, _v| false);
        assert_eq!(not_in_map, None);
    }

    #[test]
    fn test_try_get() {
        {
            let map = ClashMap::new();
            map.insert("Johnny", 21);

            assert_eq!(*map.try_get("Johnny").unwrap(), 21);

            let _result1_locking = map.get_mut("Johnny");

            let result2 = map.try_get("Johnny");
            assert!(result2.is_locked());
        }

        {
            let map = ClashMap::new();
            map.insert("Johnny", 21);

            *map.try_get_mut("Johnny").unwrap() += 1;
            assert_eq!(*map.get("Johnny").unwrap(), 22);

            let _result1_locking = map.get("Johnny");

            let result2 = map.try_get_mut("Johnny");
            assert!(result2.is_locked());
        }
    }

    #[test]
    fn test_try_reserve() {
        let mut map: ClashMap<i32, i32> = ClashMap::new();
        // ClashMap is empty and doesn't allocate memory
        assert_eq!(map.capacity(), 0);

        map.try_reserve(10).unwrap();

        // And now map can hold at least 10 elements
        assert!(map.capacity() >= 10);
    }

    #[test]
    fn test_try_reserve_errors() {
        let mut map: ClashMap<i32, i32> = ClashMap::new();

        match map.try_reserve(usize::MAX) {
            Err(_) => {}
            _ => panic!("should have raised CapacityOverflow error"),
        }
    }
}
