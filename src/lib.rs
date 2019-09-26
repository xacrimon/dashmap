//! Please see the struct level documentation.

mod fut_rwlock;
mod util;

use ahash::ABuildHasher;
use fut_rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use futures::future::{Future, FutureExt};
use hashbrown::HashMap;
use owning_ref::{OwningRef, OwningRefMut};
use std::borrow::Borrow;
use std::convert::TryInto;
use std::hash::{BuildHasher, Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use std::fmt::{self, Debug};
use util::map_in_place;

/// DashMap is a threadsafe, versatile and concurrent hashmap with good performance and is balanced for both reads and writes.
///
/// The API mostly matches that of the standard library hashmap but there are some
/// differences to due to the design.
///
/// One of those limits is iteration, you cannot iterate over the elements directly.
/// Instead you have to iterate over chunks which can iterate over KV pairs.
/// This is needed in order to use the calling thread stack as scratch space to avoid heap allocations.
///
/// The iter method currently provides a more ergonomic iterator that is slightly less performant.
/// It should be extremely performant still but it is a tad slower than using the chunks interface.
///
/// Unsafe is used to avoid bounds checking when accessing chunks.
/// This is guaranteed to be safe since we cannot possibly get a value higher than the amount of chunks.
/// The amount of chunks cannot be altered after creation in any way.
///
/// This map is not lockfree but uses some clever locking internally. It has good average case performance
///
/// You should not rely on being able to hold any combination of references involving a mutable one as it may cause a deadlock.
/// This will be fixed in the future.
pub struct DashMap<K, V>
where
    K: Hash + Eq,
{
    ncb: usize,
    submaps: Box<[RwLock<HashMap<K, V>>]>,
    hash_builder: ABuildHasher,
}

impl<'a, K: 'a, V: 'a> DashMap<K, V>
where
    K: Hash + Eq,
{
    /// Create a new DashMap.
    /// If you do not have specific requirements and understand the code you should probably call `DashMap::default` instead. It will determine
    /// the optimal parameters automagically.
    /// The amount of chunks used is based on the formula 2^n where n is the value passed. The default method will automagically determine the optimal amount.
    ///
    /// Will panic if the first parameter plugged into the formula 2^n produces a result higher than isize::MAX.
    pub fn new(num_chunks_log_2: u8) -> Self {
        let ncm = 1 << num_chunks_log_2 as usize;

        Self {
            ncb: num_chunks_log_2 as usize,
            submaps: (0..ncm)
                .map(|_| RwLock::new(HashMap::new()))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            hash_builder: ABuildHasher::new(),
        }
    }

    /// Create a new DashMap with a specified capacity.
    ///
    /// Will panic if the first parameter plugged into the formula 2^n produces a result higher than isize::MAX.
    pub fn with_capacity(num_chunks_log_2: u8, capacity: usize) -> Self {
        let ncm = 1 << num_chunks_log_2 as usize;
        let cpm = capacity / ncm;

        Self {
            ncb: num_chunks_log_2 as usize,
            submaps: (0..ncm)
                .map(|_| RwLock::new(HashMap::with_capacity(cpm)))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            hash_builder: ABuildHasher::new(),
        }
    }

    /// Insert an element into the map.
    #[inline]
    pub fn insert(&self, key: K, value: V) {
        let mapi = self.determine_map(&key);
        let mut submap = unsafe { self.submaps.get_unchecked(mapi).write() };
        submap.insert(key, value);
    }

    /// Get or insert an element into the map if one does not exist.
    #[inline]
    pub fn get_or_insert(&'a self, key: &K, default: V) -> DashMapRefAny<'a, K, V>
    where
        K: Clone,
    {
        let key = key.borrow();

        let mapi = self.determine_map(key);
        {
            let submap = unsafe { self.submaps.get_unchecked(mapi).read() };
            if submap.contains_key(key) {
                let or = OwningRef::new(submap);
                let or = or.map(|v| v.get(key).unwrap());
                return DashMapRefAny::Shared(DashMapRef { ptr: or });
            }
        }
        let mut submap = unsafe { self.submaps.get_unchecked(mapi).write() };
        if !submap.contains_key(key) {
            submap.insert(key.clone(), default);
        }
        let or = OwningRefMut::new(submap);
        let or = or.map_mut(|v| v.get_mut(key).unwrap());
        DashMapRefAny::Unique(DashMapRefMut { ptr: or })
    }

    /// Get or insert an element into the map if one does not exist.
    #[inline]
    pub fn get_or_insert_with<F: FnOnce() -> V>(
        &'a self,
        key: &K,
        default: F,
    ) -> DashMapRefAny<'a, K, V>
    where
        K: Clone,
    {
        let mapi = self.determine_map(key);
        {
            let submap = unsafe { self.submaps.get_unchecked(mapi).read() };
            if submap.contains_key(key) {
                let or = OwningRef::new(submap);
                let or = or.map(|v| v.get(key).unwrap());
                return DashMapRefAny::Shared(DashMapRef { ptr: or });
            }
        }
        let mut submap = unsafe { self.submaps.get_unchecked(mapi).write() };
        if !submap.contains_key(key) {
            submap.insert(key.clone(), default());
        }
        let or = OwningRefMut::new(submap);
        let or = or.map_mut(|v| v.get_mut(key).unwrap());
        DashMapRefAny::Unique(DashMapRefMut { ptr: or })
    }

    /// Check if the map contains the specified key.
    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(key);
        let submap = unsafe { self.submaps.get_unchecked(mapi).read() };
        submap.contains_key(&key)
    }

    #[inline]
    pub fn get_raw_from_key<Q>(&'a self, key: &Q) -> RwLockReadGuard<'a, HashMap<K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(key);
        unsafe { self.submaps.get_unchecked(mapi).read() }
    }

    #[inline]
    pub fn get_raw_mut_from_key<Q>(&'a self, key: &Q) -> RwLockWriteGuard<'a, HashMap<K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(key);
        unsafe { self.submaps.get_unchecked(mapi).write() }
    }

    /// Get a shared reference to an element contained within the map.
    #[inline]
    pub fn get<Q>(&'a self, key: &Q) -> Option<DashMapRef<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(key);
        let submap = unsafe { self.submaps.get_unchecked(mapi).read() };
        if submap.contains_key(&key) {
            let or = OwningRef::new(submap);
            let or = or.map(|v| v.get(key).unwrap());
            Some(DashMapRef { ptr: or })
        } else {
            None
        }
    }

    #[inline]
    pub fn async_get<Q>(&'a self, key: Q) -> impl Future<Output = Option<DashMapRef<'a, K, V>>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + Sized,
    {
        let mapi = self.determine_map(&key);
        let submapfut = unsafe { self.submaps.get_unchecked(mapi).async_read() };
        submapfut.map(move |submap| {
            if submap.contains_key(&key) {
                let or = OwningRef::new(submap);
                let or = or.map(|v| v.get(&key).unwrap());
                Some(DashMapRef { ptr: or })
            } else {
                None
            }
        })
    }

    /// Same as above but will return an error if the method would block at the current time.
    #[inline]
    pub fn try_get<Q>(&'a self, key: &Q) -> TryGetResult<DashMapRef<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(&key);
        if let Some(submap) = unsafe { self.submaps.get_unchecked(mapi).try_read() } {
            if submap.contains_key(&key) {
                let or = OwningRef::new(submap);
                let or = or.map(|v| v.get(key).unwrap());
                Ok(DashMapRef { ptr: or })
            } else {
                Err(TryGetError::InvalidKey)
            }
        } else {
            Err(TryGetError::WouldBlock)
        }
    }

    /// Same as above but will return an error if the method would block at the current time.
    #[inline]
    pub fn try_get_with_timeout<Q>(
        &'a self,
        key: &Q,
        timeout: Duration,
    ) -> TryGetResult<DashMapRef<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(&key);
        if let Some(submap) = unsafe { self.submaps.get_unchecked(mapi).try_read_for(timeout) } {
            if submap.contains_key(&key) {
                let or = OwningRef::new(submap);
                let or = or.map(|v| v.get(key).unwrap());
                Ok(DashMapRef { ptr: or })
            } else {
                Err(TryGetError::InvalidKey)
            }
        } else {
            Err(TryGetError::DidNotResolve)
        }
    }

    /// Shortcut for a get followed by an unwrap.
    #[inline]
    pub fn index<Q>(&'a self, key: &Q) -> DashMapRef<'a, K, V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(key).expect("Key did not exist in map")
    }

    /// Get a unique reference to an element contained within the map.
    #[inline]
    pub fn get_mut<Q>(&'a self, key: &Q) -> Option<DashMapRefMut<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(&key);
        let submap = unsafe { self.submaps.get_unchecked(mapi).write() };
        if submap.contains_key(&key) {
            let or = OwningRefMut::new(submap);
            let or = or.map_mut(|v| v.get_mut(key).unwrap());
            Some(DashMapRefMut { ptr: or })
        } else {
            None
        }
    }

    #[inline]
    pub fn async_get_mut<Q>(
        &'a self,
        key: Q,
    ) -> impl Future<Output = Option<DashMapRefMut<'a, K, V>>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + Sized,
    {
        let mapi = self.determine_map(&key);
        let submapfut = unsafe { self.submaps.get_unchecked(mapi).async_write() };
        submapfut.map(move |submap| {
            if submap.contains_key(&key) {
                let or = OwningRefMut::new(submap);
                let or = or.map_mut(|v| v.get_mut(&key).unwrap());
                Some(DashMapRefMut { ptr: or })
            } else {
                None
            }
        })
    }

    /// Same as above but will return an error if the method would block at the current time.
    #[inline]
    pub fn try_get_mut<Q>(&'a self, key: &Q) -> TryGetResult<DashMapRefMut<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(&key);
        if let Some(submap) = unsafe { self.submaps.get_unchecked(mapi).try_write() } {
            if submap.contains_key(&key) {
                let or = OwningRefMut::new(submap);
                let or = or.map_mut(|v| v.get_mut(key).unwrap());
                Ok(DashMapRefMut { ptr: or })
            } else {
                Err(TryGetError::InvalidKey)
            }
        } else {
            Err(TryGetError::WouldBlock)
        }
    }

    /// Same as above but will return an error if the method would block at the current time.
    #[inline]
    pub fn try_get_mut_with_timeout<Q>(
        &'a self,
        key: &Q,
        timeout: Duration,
    ) -> TryGetResult<DashMapRefMut<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(&key);
        if let Some(submap) = unsafe { self.submaps.get_unchecked(mapi).try_write_for(timeout) } {
            if submap.contains_key(&key) {
                let or = OwningRefMut::new(submap);
                let or = or.map_mut(|v| v.get_mut(key).unwrap());
                Ok(DashMapRefMut { ptr: or })
            } else {
                Err(TryGetError::InvalidKey)
            }
        } else {
            Err(TryGetError::DidNotResolve)
        }
    }

    /// Shortcut for a get_mut followed by an unwrap.
    #[inline]
    pub fn index_mut<Q>(&'a self, key: &Q) -> DashMapRefMut<'a, K, V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get_mut(key).expect("Key did not exist in map")
    }

    /// Get the amount of elements stored within the map.
    #[inline]
    pub fn len(&self) -> usize {
        self.submaps.iter().map(|s| s.read().len()).sum()
    }

    /// Check if the map is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Remove an element from the map if it exists. Will return the K, V pair.
    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mapi = self.determine_map(&key);
        let mut submap = unsafe { self.submaps.get_unchecked(mapi).write() };
        submap.remove_entry(key)
    }

    /// Retain all elements that the specified function returns `true` for.
    #[inline]
    pub fn retain<F: Clone + FnMut(&K, &mut V) -> bool>(&self, f: F) {
        self.submaps.iter().for_each(|locked| {
            let mut submap = locked.write();
            submap.retain(f.clone());
        });
    }

    /// Clear all elements from the map.
    #[inline]
    pub fn clear(&self) {
        self.submaps.iter().for_each(|locked| {
            let mut submap = locked.write();
            submap.clear();
        });
    }

    /// Apply a function to a a specified entry in the map.
    #[inline]
    pub fn alter<Q, F: FnOnce(V) -> V>(&self, k: &Q, f: F)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let v = self.get_mut(k);
        if let Some(mut v) = v {
            unsafe {
                map_in_place(&mut *v, f);
            }
        }
    }

    /// Apply a function to every item in the map.
    #[inline]
    pub fn alter_all<F: FnMut(V) -> V + Clone>(&self, f: F) {
        self.chunks_write().for_each(|mut t| {
            t.iter_mut().for_each(|v| unsafe {
                map_in_place(&mut *v.1, f.clone());
            })
        });
    }

    /// Iterate over the (K, V) pairs stored in the map immutably.
    #[inline]
    pub fn iter(&'a self) -> Iter<'a, K, V> {
        Iter::new(self)
    }

    /// Iterate over the (K, V) pairs stored in the map mutably.
    #[inline]
    pub fn iter_mut(&'a self) -> IterMut<'a, K, V> {
        IterMut::new(self)
    }

    /// Iterate over chunks in a read only fashion.
    #[inline]
    pub fn chunks(&self) -> impl Iterator<Item = Chunk<K, V>> {
        self.submaps.iter().map(|t| Chunk::new(t.read()))
    }

    /// Iterate over chunks in a read-write fashion.
    #[inline]
    pub fn chunks_write(&self) -> impl Iterator<Item = ChunkMut<K, V>> {
        self.submaps.iter().map(|t| ChunkMut::new(t.write()))
    }

    #[inline]
    pub(crate) fn determine_map<Q>(&self, key: &Q) -> usize
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut hash_state = self.hash_builder.build_hasher();
        key.hash(&mut hash_state);

        let hash = hash_state.finish();
        let shift = util::ptr_size_bits() - self.ncb;

        (hash >> shift) as usize
    }

    #[inline]
    pub fn chunks_count(&self) -> usize {
        self.submaps.len()
    }
}

impl<K, V> Default for DashMap<K, V>
where
    K: Hash + Eq,
{
    /// Creates a new DashMap and automagically determines the optimal amount of chunks.
    fn default() -> Self {
        let vcount = num_cpus::get() * 4;

        let base: usize = 2;
        let mut p2exp: u32 = 1;

        loop {
            if vcount <= base.pow(p2exp) {
                return Self::new(p2exp.try_into().unwrap());
            } else {
                p2exp += 1;
            }
        }
    }
}

impl<K, V> Debug for DashMap<K, V>
where
    K: Eq + Hash + Debug,
    V: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entries_iter = self.iter().map(|entry| (entry.ptr_k, entry.ptr_v));
        f.debug_map().entries(entries_iter).finish()
    }
}


/// A shared reference into a DashMap created from an iterator.
pub struct DashMapIterRef<'a, K, V>
where
    K: Hash + Eq,
{
    guard: Option<Arc<RwLockReadGuard<'a, HashMap<K, V>>>>,
    ptr_k: &'a K,
    ptr_v: &'a V,
}

impl<'a, K, V> DashMapIterRef<'a, K, V>
where
    K: Hash + Eq,
{
    /// Get the key of the entry.
    #[inline]
    pub fn key(&self) -> &K {
        self.ptr_k
    }

    /// Get the value of the entry.
    #[inline]
    pub fn value(&self) -> &V {
        self.ptr_v
    }
}

impl<'a, K, V> Drop for DashMapIterRef<'a, K, V>
where
    K: Hash + Eq,
{
    #[inline]
    fn drop(&mut self) {
        self.guard.take();
    }
}

impl<'a, K, V> Deref for DashMapIterRef<'a, K, V>
where
    K: Hash + Eq,
{
    type Target = V;

    #[inline]
    fn deref(&self) -> &V {
        self.ptr_v
    }
}

/// An immutable iterator over a DashMap.
#[allow(clippy::type_complexity)]
pub struct Iter<'a, K, V>
where
    K: Hash + Eq,
{
    c_map_index: usize,
    map: &'a DashMap<K, V>,
    c_iter: Option<(
        Arc<RwLockReadGuard<'a, HashMap<K, V>>>,
        hashbrown::hash_map::Iter<'a, K, V>,
    )>,
}

impl<'a, K, V> Iter<'a, K, V>
where
    K: Hash + Eq,
{
    fn new(map: &'a DashMap<K, V>) -> Self {
        Self {
            c_map_index: 0,
            map,
            c_iter: None,
        }
    }

    fn slow_path_new_chunk(&mut self) -> Option<DashMapIterRef<'a, K, V>> {
        if self.c_map_index == self.map.submaps.len() {
            return None;
        }

        let guard = Arc::into_raw(Arc::new(self.map.submaps[self.c_map_index].read()));
        let iter = unsafe { (&*guard).iter() };

        std::mem::replace(
            &mut self.c_iter,
            Some((unsafe { Arc::from_raw(guard) }, iter)),
        );

        self.c_map_index += 1;
        self.next()
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V>
where
    K: Hash + Eq,
{
    type Item = DashMapIterRef<'a, K, V>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c_iter) = &mut self.c_iter {
            if let Some(i) = c_iter.1.next() {
                let guard = Some(c_iter.0.clone());
                let ptr_k = unsafe { &*(i.0 as *const _) };
                let ptr_v = unsafe { &*(i.1 as *const _) };

                return Some(DashMapIterRef {
                    guard,
                    ptr_k,
                    ptr_v,
                });
            }
        }

        self.slow_path_new_chunk()
    }
}

/// A shared reference into a DashMap created from an iterator.
pub struct DashMapIterRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    guard: Option<Arc<RwLockWriteGuard<'a, HashMap<K, V>>>>,
    ptr_k: &'a K,
    ptr_v: &'a mut V,
}

impl<'a, K, V> DashMapIterRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    /// Get the key of the entry.
    #[inline]
    pub fn key(&self) -> &K {
        self.ptr_k
    }

    /// Get the value of the entry.
    #[inline]
    pub fn value(&mut self) -> &mut V {
        self.ptr_v
    }
}

impl<'a, K, V> Drop for DashMapIterRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    #[inline]
    fn drop(&mut self) {
        self.guard.take();
    }
}

impl<'a, K, V> Deref for DashMapIterRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    type Target = V;

    #[inline]
    fn deref(&self) -> &V {
        self.ptr_v
    }
}

impl<'a, K, V> DerefMut for DashMapIterRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut V {
        self.ptr_v
    }
}

/// An mutable iterator over a DashMap.
#[allow(clippy::type_complexity)]
pub struct IterMut<'a, K, V>
where
    K: Hash + Eq,
{
    c_map_index: usize,
    map: &'a DashMap<K, V>,
    c_iter: Option<(
        Arc<RwLockWriteGuard<'a, HashMap<K, V>>>,
        hashbrown::hash_map::IterMut<'a, K, V>,
    )>,
}

impl<'a, K, V> IterMut<'a, K, V>
where
    K: Hash + Eq,
{
    fn new(map: &'a DashMap<K, V>) -> Self {
        Self {
            c_map_index: 0,
            map,
            c_iter: None,
        }
    }

    fn slow_path_new_chunk(&mut self) -> Option<DashMapIterRefMut<'a, K, V>> {
        if self.c_map_index == self.map.submaps.len() {
            return None;
        }

        let guard: *mut RwLockWriteGuard<'_, HashMap<K, V>> =
            Arc::into_raw(Arc::new(self.map.submaps[self.c_map_index].write())) as _;
        let gr: &mut RwLockWriteGuard<'_, HashMap<K, V>> = unsafe { &mut *guard };
        let iter = gr.iter_mut();

        std::mem::replace(
            &mut self.c_iter,
            Some((unsafe { Arc::from_raw(guard) }, iter)),
        );

        self.c_map_index += 1;
        self.next()
    }
}

impl<'a, K, V> Iterator for IterMut<'a, K, V>
where
    K: Hash + Eq,
{
    type Item = DashMapIterRefMut<'a, K, V>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c_iter) = &mut self.c_iter {
            if let Some(i) = c_iter.1.next() {
                let guard = Some(c_iter.0.clone());
                let ptr_k = unsafe { &*(i.0 as *const _) };
                let ptr_v = unsafe { &mut *(i.1 as *mut _) };

                return Some(DashMapIterRefMut {
                    guard,
                    ptr_k,
                    ptr_v,
                });
            }
        }

        self.slow_path_new_chunk()
    }
}

/// A read only iterator interface to a chunk.
pub struct Chunk<'a, K, V>
where
    K: Hash + Eq,
{
    inner: RwLockReadGuard<'a, HashMap<K, V>>,
}

impl<'a, K: 'a, V: 'a> Chunk<'a, K, V>
where
    K: Hash + Eq,
{
    #[inline]
    fn new(inner: RwLockReadGuard<'a, HashMap<K, V>>) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }
}

/// A read-write iterator interface to a chunk.
pub struct ChunkMut<'a, K, V>
where
    K: Hash + Eq,
{
    inner: RwLockWriteGuard<'a, HashMap<K, V>>,
}

impl<'a, K: 'a, V: 'a> ChunkMut<'a, K, V>
where
    K: Hash + Eq,
{
    #[inline]
    fn new(inner: RwLockWriteGuard<'a, HashMap<K, V>>) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.inner.iter_mut()
    }
}

/// A shared reference into a DashMap.
pub struct DashMapRef<'a, K, V>
where
    K: Hash + Eq,
{
    ptr: OwningRef<RwLockReadGuard<'a, HashMap<K, V>>, V>,
}

impl<'a, K, V> Deref for DashMapRef<'a, K, V>
where
    K: Hash + Eq,
{
    type Target = V;

    #[inline]
    fn deref(&self) -> &V {
        &*self.ptr
    }
}

/// A unique reference into a DashMap.
pub struct DashMapRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    ptr: OwningRefMut<RwLockWriteGuard<'a, HashMap<K, V>>, V>,
}

impl<'a, K, V> Deref for DashMapRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    type Target = V;

    #[inline]
    fn deref(&self) -> &V {
        &*self.ptr
    }
}

impl<'a, K, V> DerefMut for DashMapRefMut<'a, K, V>
where
    K: Hash + Eq,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut V {
        &mut *self.ptr
    }
}

/// A unique reference into a DashMap.
pub enum DashMapRefAny<'a, K, V>
where
    K: Hash + Eq,
{
    Shared(DashMapRef<'a, K, V>),
    Unique(DashMapRefMut<'a, K, V>),
    Marker(PhantomData<&'a K>, PhantomData<&'a V>),
}

impl<'a, K, V> Deref for DashMapRefAny<'a, K, V>
where
    K: Hash + Eq,
{
    type Target = V;

    #[inline]
    fn deref(&self) -> &V {
        match self {
            DashMapRefAny::Shared(r) => &*r,
            DashMapRefAny::Unique(r) => &*r,
            DashMapRefAny::Marker(_, _) => unreachable!(),
        }
    }
}

/// A error possibly returned by the try_get family of methods for DashMap.
pub enum TryGetError {
    /// Returned if the key did not exist in the map.
    InvalidKey,

    /// Returned if the operation was going to block.
    WouldBlock,

    /// Returned if the lock did not become available within the specified timeout.
    DidNotResolve,
}

/// Alias for a Result with TryGetError as it's error type.
pub type TryGetResult<T> = Result<T, TryGetError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn use_map(mut e: DashMapRefMut<i32, i32>) {
        *e *= 2;
    }

    #[test]
    fn move_deref() {
        let map = DashMap::default();
        map.insert(3, 69);
        let e = map.index_mut(&3);
        use_map(e);
        println!("e: {}", *map.index_mut(&3));
    }

    #[test]
    fn insert_then_assert_1024() {
        let map = DashMap::default();

        for i in 0..1024_i32 {
            map.insert(i, i * 2);
        }

        map.alter_all(|v| v * 2);

        for i in 0..1024_i32 {
            assert_eq!(i * 4, *map.get(&i).unwrap());
        }
    }

    #[test]
    fn insert_then_iter_1024() {
        let map = DashMap::default();

        for i in 0..1024_i32 {
            map.insert(i, i * 2);
        }

        map.alter_all(|v| v * 2);

        assert_eq!(map.iter().count(), 1024);
    }

    #[test]
    fn insert_then_iter_mut_map_1024() {
        let map = DashMap::default();

        for i in 0..1024_i32 {
            map.insert(i, 4);
        }

        map.iter_mut().for_each(|mut r| *r *= 2);

        assert_eq!(map.iter().fold(0, |acc, e| acc + *e), 8192);
    }

    #[test]
    fn insert_then_assert_str() {
        let map = DashMap::default();
        map.insert("foo".to_string(), 51i32);
        assert_eq!(*map.index("foo"), 51i32);
    }

    #[test]
    fn match_debug() {
        let map = DashMap::default();
        map.insert(1i32, 2i32);
        map.insert(3i32, 6i32);

        let choices = [
            "{1: 2, 3: 6}",
            "{3: 6, 1: 2}",
        ];

        let map_debug = format!("{:?}", map);

        for choice in &choices {
            if map_debug == *choice { return }
        }

        panic!("no match\n{}", map_debug);
    }
}
