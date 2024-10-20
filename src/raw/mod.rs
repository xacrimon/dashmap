mod utils;
mod parker;
mod alloc;
mod probe;

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::mem::{self, MaybeUninit};
use std::sync::atomic::{fence, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::{hint, panic, ptr};

use self::alloc::RawTable;
use probe::Probe;
use utils::{untagged, AtomicPtrFetchOps, Counter, Shared, StrictProvenance, Tagged};
use crate::map::{Compute, Operation, ResizeMode};
use parker::Parker;

use seize::{AsLink, Collector, Guard, Link};

// A lock-free hash-table.
pub struct HashMap<K, V, S> {
    // A pointer to the root table.
    table: AtomicPtr<RawTable>,
    // Collector for memory reclamation.
    //
    // The collector is allocated as it's aliased by each table,
    // in case it needs to be accessed during reclamation.
    collector: Shared<Collector>,
    // The resize mode, either blocking or incremental.
    resize: ResizeMode,
    // The number of keys in the table.
    count: Counter,
    // Hasher for keys.
    pub hasher: S,
    _kv: PhantomData<(K, V)>,
}

// The hash-table allocation.
type Table<K, V> = self::alloc::Table<Entry<K, V>>;

// Hash-table state.
pub struct State {
    // The next table used for resizing.
    pub next: AtomicPtr<RawTable>,
    // A lock acquired to allocate the next table.
    pub allocating: Mutex<()>,
    // The number of entries that have been copied to the next table.
    pub copied: AtomicUsize,
    // The number of entries that have been claimed by copiers,
    // but not necessarily copied.
    pub claim: AtomicUsize,
    // The status of the resize.
    pub status: AtomicU32,
    // A thread parker for blocking on copy operations.
    pub parker: Parker,
    // Entries whose retirement has been deferred by later tables.
    pub deferred: seize::Deferred,
    // A pointer to the root collector, valid as long as the map is alive.
    pub collector: *const Collector,
}

impl Default for State {
    fn default() -> State {
        State {
            next: AtomicPtr::new(ptr::null_mut()),
            allocating: Mutex::new(()),
            copied: AtomicUsize::new(0),
            claim: AtomicUsize::new(0),
            status: AtomicU32::new(State::PENDING),
            parker: Parker::default(),
            deferred: seize::Deferred::new(),
            collector: ptr::null(),
        }
    }
}

impl State {
    // A resize is in-progress.
    pub const PENDING: u32 = 0;
    // The resize has been aborted, continue to the next table.
    pub const ABORTED: u32 = 1;
    // The resize was complete and the table was promoted.
    pub const PROMOTED: u32 = 2;
}

// The result of an insert operation.
pub enum InsertResult<'g, V> {
    // Inserted the given value.
    Inserted(&'g V),
    // Replaced the given value.
    Replaced(&'g V),
    // Error returned by `try_insert`.
    Error { current: &'g V, not_inserted: V },
}

// The raw result of an insert operation.
pub enum RawInsertResult<'g, K, V> {
    // Inserted the given value.
    Inserted(&'g V),
    // Replaced the given value.
    Replaced(&'g V),
    // Error returned by `try_insert`.
    Error {
        current: Tagged<Entry<K, V>>,
        not_inserted: *mut Entry<K, V>,
    },
}

// An entry in the hash-table.
#[repr(C)]
pub struct Entry<K, V> {
    pub link: Link,
    pub key: K,
    pub value: V,
}

// Safety: repr(C) and seize::Link is the first field
unsafe impl<K, V> AsLink for Entry<K, V> {}

impl Entry<(), ()> {
    // The entry is being copied to the new table, no updates are allowed on the old table.
    //
    // This bit is put down to initiate a copy, forcing all writers to complete the resize
    // before making progress.
    const COPYING: usize = 0b001;

    // The entry has been copied to the new table.
    //
    // This bit is put down after a copy completes. Both readers and writers must go to
    // the new table to see the new state of the entry.
    //
    // In blocking mode this is unused.
    const COPIED: usize = 0b010;

    // The entry was copied from a previous table.
    //
    // This bit indicates that an entry may still be accessible from previous tables
    // because the resize is still in progress, and so it is unsafe to reclaim.
    //
    // In blocking mode this is unused.
    const BORROWED: usize = 0b100;

    // Reclaims an entry.
    #[inline]
    unsafe fn reclaim<K, V>(link: *mut Link) {
        let entry: *mut Entry<K, V> = link.cast();
        let _entry = unsafe { Box::from_raw(entry) };
    }
}

impl<K, V> utils::Unpack for Entry<K, V> {
    // Mask for an entry pointer, ignoring any tag bits.
    const MASK: usize = !(Entry::COPYING | Entry::COPIED | Entry::BORROWED);
}

impl<K, V> Entry<K, V> {
    // A sentinel pointer for a deleted entry.
    //
    // Null pointers are never copied to the new table, so this state is safe to use.
    // Note that tombstone entries may still be marked as `COPYING`, so this state
    // cannot be used for direct equality.
    const TOMBSTONE: *mut Entry<K, V> = Entry::COPIED as _;
}

/// The status of an entry.
enum EntryStatus<K, V> {
    // The entry is a tombstone or null (potentially a null copy).
    Null,
    // The entry is being copied.
    Copied(Tagged<Entry<K, V>>),
    // A valid entry.
    Value(Tagged<Entry<K, V>>),
}

impl<K, V> From<Tagged<Entry<K, V>>> for EntryStatus<K, V> {
    #[inline]
    fn from(entry: Tagged<Entry<K, V>>) -> Self {
        if entry.ptr.is_null() {
            EntryStatus::Null
        } else if entry.tag() & Entry::COPYING != 0 {
            EntryStatus::Copied(entry)
        } else {
            EntryStatus::Value(entry)
        }
    }
}

/// The state of an entry we attempted to update.
enum UpdateStatus<K, V> {
    // Successfully replaced the given key and value.
    Replaced(Tagged<Entry<K, V>>),
    // A new entry was written before we could update.
    Found(EntryStatus<K, V>),
}

/// The state of an entry we attempted to insert into.
enum InsertStatus<K, V> {
    // Successfully inserted the value.
    Inserted,
    // A new entry was written before we could update.
    Found(EntryStatus<K, V>),
}

impl<K, V, S> HashMap<K, V, S> {
    // Creates new hash-table with the given options.
    #[inline]
    pub fn new(
        capacity: usize,
        hasher: S,
        collector: Collector,
        resize: ResizeMode,
    ) -> HashMap<K, V, S> {
        let collector = Shared::from(collector);

        // The table is lazily allocated.
        if capacity == 0 {
            return HashMap {
                collector,
                resize,
                hasher,
                table: AtomicPtr::new(ptr::null_mut()),
                count: Counter::default(),
                _kv: PhantomData,
            };
        }

        // Initialize the table and mark it as the root.
        let mut table = Table::<K, V>::alloc(probe::entries_for(capacity), &collector);
        *table.state_mut().status.get_mut() = State::PROMOTED;

        HashMap {
            hasher,
            resize,
            collector,
            table: AtomicPtr::new(table.raw),
            count: Counter::default(),
            _kv: PhantomData,
        }
    }

    // Returns a reference to the root hash-table.
    #[inline]
    pub fn root<'g>(&self, guard: &'g impl Guard) -> HashMapRef<'g, K, V, S> {
        assert!(
            guard.belongs_to(&self.collector),
            "accessed map with incorrect guard"
        );

        // Safety: verified that the guard belongs to our collector.
        unsafe { self.root_unchecked(guard) }
    }

    // Returns a reference to the root hash-table.
    //
    // # Safety
    //
    // The guard must belong to this table's collector.
    #[inline]
    pub unsafe fn root_unchecked<'g>(&self, guard: &'g impl Guard) -> HashMapRef<'g, K, V, S> {
        // Load the root table.
        let raw = guard.protect(&self.table, Ordering::Acquire);
        let table = unsafe { Table::<K, V>::from_raw(raw) };

        // Safety: The caller guarantees that the guard comes from our collector, so
        // &'g Guard implies &'g self. This makes bounds a little nicer for users.
        unsafe {
            mem::transmute::<HashMapRef<'_, K, V, S>, HashMapRef<'g, K, V, S>>(self.as_ref(table))
        }
    }

    // Returns a reference to the collector.
    #[inline]
    pub fn collector(&self) -> &Collector {
        &self.collector
    }

    // Returns the number of entries in the table.
    #[inline]
    pub fn len(&self) -> usize {
        self.count.sum()
    }

    // Returns true if incremental resizing is enabled.
    #[inline]
    fn is_incremental(&self) -> bool {
        matches!(self.resize, ResizeMode::Incremental(_))
    }

    // Returns a reference to the given table.
    #[inline]
    fn as_ref(&self, table: Table<K, V>) -> HashMapRef<'_, K, V, S> {
        HashMapRef { table, root: self }
    }
}

// A reference to the root table, or an arbitrarily nested table migration.
pub struct HashMapRef<'a, K, V, S> {
    table: Table<K, V>,
    root: &'a HashMap<K, V, S>,
}

// Hash-table operations.
impl<'root, K, V, S> HashMapRef<'root, K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    // Returns a reference to the entry corresponding to the key.
    #[inline]
    pub fn get<'g, Q>(&self, key: &Q, guard: &'g impl Guard) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // The table has not yet been allocated.
        if self.table.raw.is_null() {
            return None;
        }

        // Initialize the probe state.
        let (h1, h2) = self.hash(key);
        let mut probe = Probe::start(h1, self.table.mask);

        // Probe until we reach the limit.
        while probe.len <= self.table.limit {
            // Load the entry metadata first for cheap searches.
            let meta = unsafe { self.table.meta(probe.i) }.load(Ordering::Acquire);

            // The key is not in the table.
            //
            // It also cannot be in the next table because we have not went over the probe limit.
            if meta == meta::EMPTY {
                return None;
            }

            // Check for a potential match.
            if meta != h2 {
                probe.next(self.table.mask);
                continue;
            }

            // Load the full entry.
            let entry =
                unsafe { guard.protect(self.table.entry(probe.i), Ordering::Acquire) }.unpack();

            // The entry was deleted, keep probing.
            if entry.ptr.is_null() {
                probe.next(self.table.mask);
                continue;
            }

            // Check for a full match.
            if unsafe { (*entry.ptr).key.borrow() } == key {
                // The entry was copied to the new table.
                //
                // In blocking resize mode we do not need to perform this check as all writes block
                // until any resizes are complete, making the root table the source of truth for readers.
                if entry.tag() & Entry::COPIED != 0 {
                    break;
                }

                // Found the correct entry, return the key and value.
                return unsafe { Some((&(*entry.ptr).key, &(*entry.ptr).value)) };
            }

            probe.next(self.table.mask);
        }

        // In incremental resize mode, we have to check the next table if we found
        // a copied entry or went over the probe limit.
        if self.root.is_incremental() {
            if let Some(next) = self.next_table_ref() {
                // Retry in the new table.
                return next.get(key, guard);
            }
        }

        // Otherwise, the key is not in the table.
        None
    }

    // Inserts a key-value pair into the table.
    #[inline]
    pub fn insert<'g>(
        &mut self,
        key: K,
        value: V,
        replace: bool,
        guard: &'g impl Guard,
    ) -> InsertResult<'g, V> {
        // Allocate the entry to be inserted.
        let entry = Box::into_raw(Box::new(Entry {
            key,
            value,
            link: self.root.collector.link(guard),
        }));

        // Perform the insert.
        //
        // Safety: We just allocated the entry above.
        let result = unsafe { self.insert_with(untagged(entry), replace, true, guard) };
        let result = match result {
            RawInsertResult::Inserted(value) => InsertResult::Inserted(value),
            RawInsertResult::Replaced(value) => InsertResult::Replaced(value),
            RawInsertResult::Error {
                current,
                not_inserted,
            } => {
                let current = unsafe { &(*current.ptr).value };

                // Safety: We allocated this box above and it was not inserted into the table.
                let not_inserted = unsafe { Box::from_raw(not_inserted) };

                InsertResult::Error {
                    current,
                    not_inserted: not_inserted.value,
                }
            }
        };

        // Increment the length if we inserted a new entry.
        if matches!(result, InsertResult::Inserted(_)) {
            self.root
                .count
                .get(guard.thread().id())
                .fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    // Inserts an entry into the map.
    //
    // # Safety
    //
    // The new entry must be a valid pointer.
    #[inline]
    unsafe fn insert_with<'g>(
        &mut self,
        new_entry: Tagged<Entry<K, V>>,
        should_replace: bool,
        help_copy: bool,
        guard: &'g impl Guard,
    ) -> RawInsertResult<'g, K, V> {
        // Allocate the table if it has not been initialized yet.
        if self.table.raw.is_null() {
            self.init(None);
        }

        // Safety: The new entry is guaranteed to be valid by the caller.
        let new_ref = unsafe { &*(new_entry).ptr };

        // Initialize the probe state.
        let (h1, h2) = self.hash(&new_ref.key);
        let mut probe = Probe::start(h1, self.table.mask);

        // Probe until we reach the limit.
        let copying = 'probe: loop {
            if probe.len > self.table.limit {
                break None;
            }

            // Load the entry metadata first for cheap searches.
            let meta = unsafe { self.table.meta(probe.i) }.load(Ordering::Acquire);

            // The entry is empty, try to insert.
            let mut entry = if meta == meta::EMPTY {
                match self.insert_at(probe.i, h2, new_entry.raw, guard) {
                    // Successfully inserted.
                    InsertStatus::Inserted => return RawInsertResult::Inserted(&new_ref.value),

                    // Lost to a concurrent insert.
                    //
                    // If the key matches, we might be able to update the value.
                    InsertStatus::Found(EntryStatus::Value(found))
                    | InsertStatus::Found(EntryStatus::Copied(found)) => found,

                    // Otherwise, continue probing.
                    InsertStatus::Found(EntryStatus::Null) => {
                        probe.next(self.table.mask);
                        continue 'probe;
                    }
                }
            }
            // Found a potential match.
            else if meta == h2 {
                // Load the full entry.
                let found = guard
                    .protect(self.table.entry(probe.i), Ordering::Acquire)
                    .unpack();

                // The entry was deleted, keep probing.
                if found.ptr.is_null() {
                    probe.next(self.table.mask);
                    continue 'probe;
                }

                // If the key matches, we might be able to update the value.
                found
            }
            // Otherwise, continue probing.
            else {
                probe.next(self.table.mask);
                continue 'probe;
            };

            // Check for a full match.
            if unsafe { (*entry.ptr).key != new_ref.key } {
                probe.next(self.table.mask);
                continue 'probe;
            }

            // The entry is being copied to the new table.
            if entry.tag() & Entry::COPYING != 0 {
                break 'probe Some(probe.i);
            }

            // Return an error for calls to `try_insert`.
            if !should_replace {
                return RawInsertResult::Error {
                    current: entry,
                    not_inserted: new_entry.ptr,
                };
            }

            loop {
                // Try to update the value.
                match self.update_at(probe.i, entry, new_entry.raw, guard) {
                    // Successfully updated.
                    UpdateStatus::Replaced(entry) => {
                        return RawInsertResult::Replaced(&(*entry.ptr).value)
                    }

                    // The entry is being copied.
                    UpdateStatus::Found(EntryStatus::Copied(_)) => break 'probe Some(probe.i),

                    // The entry was deleted before we could update it, continue probing.
                    UpdateStatus::Found(EntryStatus::Null) => {
                        probe.next(self.table.mask);
                        continue 'probe;
                    }

                    // Someone else beat us to the update, retry.
                    UpdateStatus::Found(EntryStatus::Value(found)) => entry = found,
                }
            }
        };

        // If went over the probe limit or found a copied entry, trigger a resize.
        let mut next_table = self.get_or_alloc_next(None);

        let next_table = match self.root.resize {
            ResizeMode::Blocking => {
                // In blocking mode we must complete the resize before proceeding.
                self.help_copy(guard, false)
            }

            ResizeMode::Incremental(_) => {
                // Help out with the copy.
                if help_copy {
                    next_table = self.help_copy(guard, false);
                }

                // The entry we want to update is being copied.
                if let Some(i) = copying {
                    // Wait for the entry to be copied.
                    //
                    // We could race with the copy to insert into the table. However,
                    // this entire code path is very rare and likely to complete quickly,
                    // so blocking allows us to make copies faster.
                    self.wait_copied(i);
                }

                next_table
            }
        };

        // Insert into the next table.
        self.as_ref(next_table)
            .insert_with(new_entry, should_replace, false, guard)
    }

    // Removes a key from the map, returning the entry for the key if the key was previously in the map.
    #[inline]
    pub fn remove<'g, Q: ?Sized>(&self, key: &Q, guard: &'g impl Guard) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.remove_inner(key, true, guard)
    }

    // Removes a key from the map, returning the entry for the key if the key was previously in the map.
    //
    // This is a recursive helper for `remove_entry`.
    #[inline]
    pub fn remove_inner<'g, Q: ?Sized>(
        &self,
        key: &Q,
        help_copy: bool,
        guard: &'g impl Guard,
    ) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        if self.table.raw.is_null() {
            return None;
        }

        // Initialize the probe state.
        let (h1, h2) = self.hash(key);
        let mut probe = Probe::start(h1, self.table.mask);

        // Probe until we reach the limit.
        let copying = 'probe: loop {
            if probe.len > self.table.limit {
                break None;
            }

            // Load the entry metadata first for cheap searches.
            let meta = unsafe { self.table.meta(probe.i).load(Ordering::Acquire) };

            // The key is not in the table.
            // It also cannot be in the next table because we have not went over the probe limit.
            if meta == meta::EMPTY {
                return None;
            }

            // Check for a potential match.
            if meta != h2 {
                probe.next(self.table.mask);
                continue 'probe;
            }

            // Load the full entry.
            let mut entry =
                unsafe { guard.protect(self.table.entry(probe.i), Ordering::Acquire) }.unpack();

            // The entry was deleted, keep probing.
            if entry.ptr.is_null() {
                probe.next(self.table.mask);
                continue 'probe;
            }

            // Check for a full match.
            if unsafe { (*entry.ptr).key.borrow() != key } {
                probe.next(self.table.mask);
                continue 'probe;
            }

            // The entry is being copied to the new table, we have to complete the copy before
            // we can remove it.
            if entry.tag() & Entry::COPYING != 0 {
                break 'probe Some(probe.i);
            }

            loop {
                match unsafe { self.update_at(probe.i, entry, Entry::TOMBSTONE, guard) } {
                    // Successfully removed the entry.
                    UpdateStatus::Replaced(entry) => {
                        // Mark the entry as a tombstone.
                        //
                        // Note that this might end up being overwritten by the metadata hash
                        // if the initial insertion is lagging behind, but we avoid the RMW
                        // and sacrifice reads in the extremely rare case.
                        unsafe {
                            self.table
                                .meta(probe.i)
                                .store(meta::TOMBSTONE, Ordering::Release)
                        };

                        // Decrement the table length.
                        let count = self.root.count.get(guard.thread().id());
                        count.fetch_sub(1, Ordering::Relaxed);

                        let entry = unsafe { &(*entry.ptr) };
                        return Some((&entry.key, &entry.value));
                    }

                    // The entry is being copied to the new table, we have to complete the copy
                    // before we can remove.
                    UpdateStatus::Found(EntryStatus::Copied(_)) => break 'probe Some(probe.i),

                    // The entry was deleted.
                    //
                    // We know that at some point during our execution the key was not in the map.
                    UpdateStatus::Found(EntryStatus::Null) => return None,

                    // Lost to a concurrent update, retry.
                    UpdateStatus::Found(EntryStatus::Value(found)) => entry = found,
                }
            }
        };

        match self.root.resize {
            ResizeMode::Blocking => match copying {
                // The entry we want to remove is being copied.
                Some(_) => {
                    // In blocking mode we must complete the resize before proceeding.
                    let next_table = self.help_copy(guard, false);

                    // Continue in the new table.
                    return self.as_ref(next_table).remove_inner(key, help_copy, guard);
                }
                // If we went over the probe limit, the key is not in this table.
                None => None,
            },

            ResizeMode::Incremental(_) => {
                // The entry we want to remove is being copied.
                if let Some(i) = copying {
                    let next_table = self.next_table_ref().unwrap();

                    // Help out with the copy.
                    if help_copy {
                        self.help_copy(guard, false);
                    }

                    // Wait for the entry to be copied.
                    self.wait_copied(i);

                    // Continue in the new table.
                    return next_table.remove_inner(key, false, guard);
                }

                // In incremental resize mode, we have to check the next table if we found
                // a copied entry or went over the probe limit.
                if let Some(next_table) = self.next_table_ref() {
                    // Help out with the copy.
                    if help_copy {
                        self.help_copy(guard, false);
                    }

                    // Continue in the new table.
                    return next_table.remove_inner(key, false, guard);
                }

                // Otherwise, the key is not in the table.
                None
            }
        }
    }

    // Attempts to insert an entry at the given index.
    #[inline]
    unsafe fn insert_at(
        &self,
        i: usize,
        meta: u8,
        new_entry: *mut Entry<K, V>,
        guard: &impl Guard,
    ) -> InsertStatus<K, V> {
        let entry = unsafe { self.table.entry(i) };

        // Try to claim the empty entry.
        let found = match entry.compare_exchange(
            ptr::null_mut(),
            new_entry,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            // Successfully claimed the entry.
            Ok(_) => {
                // Update the metadata table.
                unsafe { self.table.meta(i).store(meta, Ordering::Release) };

                // Return the value we inserted.
                return InsertStatus::Inserted;
            }

            // Lost to a concurrent update.
            Err(found) => found.unpack(),
        };

        let (meta, status) = match EntryStatus::from(found) {
            EntryStatus::Value(_) | EntryStatus::Copied(_) => {
                // Protect the entry before accessing it.
                let found = guard.protect(entry, Ordering::Acquire).unpack();

                // Re-check the entry status.
                match EntryStatus::from(found) {
                    EntryStatus::Value(found) | EntryStatus::Copied(found) => {
                        // An entry was inserted, we have to hash it to get the metadata.
                        //
                        // The logic is the same for copied entries here as we have to
                        // check if the key matches and continue the update in the new table.
                        let hash = self.root.hasher.hash_one(&(*found.ptr).key);
                        (meta::h2(hash), EntryStatus::Value(found))
                    }

                    // The entry was deleted or null copied.
                    EntryStatus::Null => (meta::TOMBSTONE, EntryStatus::Null),
                }
            }

            // The entry was deleted or null copied.
            EntryStatus::Null => (meta::TOMBSTONE, EntryStatus::Null),
        };

        // Ensure the meta table is updated to keep the probe chain alive for readers.
        if self.table.meta(i).load(Ordering::Relaxed) == meta::EMPTY {
            self.table.meta(i).store(meta, Ordering::Release);
        }

        InsertStatus::Found(status)
    }

    // Attempts to replace the value of an existing entry at the given index.
    #[inline]
    unsafe fn update_at(
        &self,
        i: usize,
        current: Tagged<Entry<K, V>>,
        new_entry: *mut Entry<K, V>,
        guard: &impl Guard,
    ) -> UpdateStatus<K, V> {
        let entry = unsafe { self.table.entry(i) };

        // Try to perform the update.
        let found = match entry.compare_exchange_weak(
            current.raw,
            new_entry,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            // Successfully updated.
            Ok(_) => unsafe {
                // Safety: The old value is now unreachable from this table.
                self.defer_retire(current, guard);
                return UpdateStatus::Replaced(current);
            },

            // Lost to a concurrent update.
            Err(found) => found.unpack(),
        };

        let status = match EntryStatus::from(found) {
            EntryStatus::Value(_) => {
                // Protect the entry before accessing it.
                let found = guard.protect(entry, Ordering::Acquire).unpack();

                // Re-check the entry status.
                EntryStatus::from(found)
            }

            // The entry was copied.
            //
            // We don't need to protect the entry as we never access it,
            // we wait for it to be copied and continue in the new table.
            EntryStatus::Copied(entry) => EntryStatus::Copied(entry),

            // The entry was deleted.
            removed => removed,
        };

        UpdateStatus::Found(status)
    }

    // Reserve capacity for `additional` more elements.
    #[inline]
    pub fn reserve(&mut self, additional: usize, guard: &impl Guard) {
        // The table has not yet been allocated, try to initialize it.
        if self.table.raw.is_null() && self.init(Some(probe::entries_for(additional))) {
            return;
        }

        loop {
            let capacity =
                probe::entries_for(self.root.count.sum().checked_add(additional).unwrap());

            // We have enough capacity.
            if self.table.len() >= capacity {
                return;
            }

            // Race to allocate the new table.
            self.get_or_alloc_next(Some(capacity));

            // Force the copy to complete.
            //
            // Note that this is not strictly necessary for a `reserve` operation.
            self.table = self.help_copy(guard, true);
        }
    }

    // Remove all entries from this table.
    #[inline]
    pub fn clear(&mut self, guard: &impl Guard) {
        if self.table.raw.is_null() {
            return;
        }

        // Get a clean copy of the table to delete from.
        self.linearize(guard);

        // Note that this method is not implemented in terms of `retain(|_, _| true)` to avoid
        // loading entry metadata, as there is no need to provide consistency with `get`.
        let mut copying = false;
        'probe: for i in 0..self.table.len() {
            // Load the entry to delete.
            let mut entry =
                unsafe { guard.protect(self.table.entry(i), Ordering::Acquire) }.unpack();

            loop {
                // The entry is empty or already deleted.
                if entry.ptr.is_null() {
                    continue 'probe;
                }

                // Found a non-empty entry being copied.
                if entry.tag() & Entry::COPYING != 0 {
                    // Clear every entry in this table that we can, then deal with the copy.
                    copying = true;
                    continue 'probe;
                }

                // Try to delete the entry.
                let result = unsafe {
                    self.table.entry(i).compare_exchange(
                        entry.raw,
                        Entry::TOMBSTONE,
                        Ordering::Release,
                        Ordering::Acquire,
                    )
                };

                match result {
                    // Successfully deleted the entry.
                    Ok(_) => unsafe {
                        // Update the metadata table.
                        self.table.meta(i).store(meta::TOMBSTONE, Ordering::Release);

                        // Decrement the table length.
                        let count = self.root.count.get(guard.thread().id());
                        count.fetch_sub(1, Ordering::Relaxed);

                        // Safety: We just removed the old value from this table.
                        self.defer_retire(entry, guard);

                        continue 'probe;
                    },

                    // Lost to a concurrent update, retry.
                    Err(found) => entry = found.unpack(),
                }
            }
        }

        // A resize prevented us from deleting all the entries in this table.
        //
        // Complete the resize and retry in the new table.
        if copying {
            let next_table = self.help_copy(guard, true);
            return self.as_ref(next_table).clear(guard);
        }
    }

    // Retains only the elements specified by the predicate..
    #[inline]
    pub fn retain<F>(&mut self, mut f: F, guard: &impl Guard)
    where
        F: FnMut(&K, &V) -> bool,
    {
        if self.table.raw.is_null() {
            return;
        }

        // Get a clean copy of the table to delete from.
        self.linearize(guard);

        let mut copying = false;
        'probe: for i in 0..self.table.len() {
            // Load the entry metadata first to ensure consistency with calls to `get`
            // for entries that are retained.
            let meta = unsafe { self.table.meta(i) }.load(Ordering::Acquire);

            // The entry is empty or deleted.
            if matches!(meta, meta::EMPTY | meta::TOMBSTONE) {
                continue 'probe;
            }

            // Load the entry to delete.
            let mut entry =
                unsafe { guard.protect(self.table.entry(i), Ordering::Acquire) }.unpack();

            loop {
                // The entry is empty or already deleted.
                if entry.ptr.is_null() {
                    continue 'probe;
                }

                // Found a non-empty entry being copied.
                if entry.tag() & Entry::COPYING != 0 {
                    // Clear every entry in this table that we can, then deal with the copy.
                    copying = true;
                    continue 'probe;
                }

                let entry_ref = unsafe { &*entry.raw };

                // Should we retain this entry?
                if f(&entry_ref.key, &entry_ref.value) {
                    continue 'probe;
                }

                // Try to delete the entry.
                let result = unsafe {
                    self.table.entry(i).compare_exchange(
                        entry.raw,
                        Entry::TOMBSTONE,
                        Ordering::Release,
                        Ordering::Acquire,
                    )
                };

                match result {
                    // Successfully deleted the entry.
                    Ok(_) => unsafe {
                        // Update the metadata table.
                        self.table.meta(i).store(meta::TOMBSTONE, Ordering::Release);

                        // Decrement the table length.
                        let count = self.root.count.get(guard.thread().id());
                        count.fetch_sub(1, Ordering::Relaxed);

                        // Safety: We just removed the old value from this table.
                        self.defer_retire(entry, guard);

                        continue 'probe;
                    },

                    // Lost to a concurrent update, retry.
                    Err(found) => entry = found.unpack(),
                }
            }
        }

        // A resize prevented us from deleting all the entries in this table.
        //
        // Complete the resize and retry in the new table.
        if copying {
            let next_table = self.help_copy(guard, true);
            return self.as_ref(next_table).retain(f, guard);
        }
    }

    // Returns an iterator over the keys and values of this table.
    #[inline]
    pub fn iter<'g, G>(&mut self, guard: &'g G) -> Iter<'g, K, V, G>
    where
        G: Guard,
    {
        if self.table.raw.is_null() {
            return Iter {
                i: 0,
                guard,
                table: self.table,
            };
        }

        // Get a clean copy of the table to iterate over.
        self.linearize(guard);

        Iter {
            i: 0,
            guard,
            table: self.table,
        }
    }

    // Returns the h1 and h2 hash for the given key.
    #[inline]
    fn hash<Q>(&self, key: &Q) -> (usize, u8)
    where
        Q: Hash + ?Sized,
    {
        let hash = self.root.hasher.hash_one(key);
        (meta::h1(hash), meta::h2(hash))
    }
}

// A wrapper around a CAS function that manages the computed state.
struct ComputeState<F, K, V, T> {
    compute: F,
    insert: Option<V>,
    update: Option<CachedUpdate<K, V, T>>,
}

struct CachedUpdate<K, V, T> {
    input: *mut Entry<K, V>,
    output: Operation<V, T>,
}

impl<'g, F, K, V, T> ComputeState<F, K, V, T>
where
    F: FnMut(Option<(&'g K, &'g V)>) -> Operation<V, T>,
    K: 'g,
    V: 'g,
{
    // Create a new `ComputeState` for the given function.
    #[inline]
    fn new(compute: F) -> ComputeState<F, K, V, T> {
        ComputeState {
            compute,
            insert: None,
            update: None,
        }
    }

    // Performs a state transition.
    #[inline]
    fn next(&mut self, entry: Option<*mut Entry<K, V>>) -> Operation<V, T> {
        match entry {
            Some(entry) => match self.update.take() {
                Some(CachedUpdate { input, output }) if input == entry => output,
                _ => {
                    let entry = unsafe { &*entry };
                    (self.compute)(Some((&entry.key, &entry.value)))
                }
            },
            None => match self.insert.take() {
                Some(value) => Operation::Insert(value),
                None => (self.compute)(None),
            },
        }
    }

    // Restores the state if an operation fails.
    #[inline]
    fn restore(&mut self, input: Option<*mut Entry<K, V>>, output: Operation<V, T>) {
        match input {
            Some(input) => self.update = Some(CachedUpdate { input, output }),
            None => match output {
                Operation::Insert(value) => self.insert = Some(value),
                _ => unreachable!(),
            },
        }
    }
}

// A lazy initialized `Entry` allocation.
enum LazyEntry<K, V> {
    // An uninitialized entry, containing just the owned key.
    Uninit(K),
    // An allocated entry.
    Init(*mut Entry<K, MaybeUninit<V>>),
}

impl<K, V> LazyEntry<K, V> {
    // Returns a reference to the entry's key.
    #[inline]
    fn key(&self) -> &K {
        match self {
            LazyEntry::Uninit(key) => key,
            LazyEntry::Init(entry) => unsafe { &(**entry).key },
        }
    }

    // Initializes the entry if it has not already been initialized, returning the pointer.
    #[inline]
    fn init(&mut self, collector: &Collector, guard: &impl Guard) -> *mut Entry<K, MaybeUninit<V>> {
        match self {
            LazyEntry::Init(entry) => *entry,
            LazyEntry::Uninit(key) => {
                // Safety: we read the current key with `ptr::read` and overwrite the
                // state with `ptr::write`. We also make sure to abort if the allocator
                // panics, ensuring the current value is not dropped twice.
                unsafe {
                    let key = ptr::read(key);
                    let entry = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        Box::into_raw(Box::new(Entry {
                            link: collector.link(guard),
                            value: MaybeUninit::uninit(),
                            key,
                        }))
                    }))
                    .unwrap_or_else(|_| std::process::abort());
                    ptr::write(self, LazyEntry::Init(entry));
                    entry
                }
            }
        }
    }
}

// Update operations.
impl<'root, K, V, S> HashMapRef<'root, K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Returns a reference to the value corresponding to the key, or inserts a default value
    /// computed from a closure.
    #[inline]
    pub fn get_or_insert_with<'g, F>(&mut self, key: K, f: F, guard: &'g impl Guard) -> &'g V
    where
        F: FnOnce() -> V,
        K: 'g,
    {
        let mut f = Some(f);
        let compute = |entry| match entry {
            // Return the existing value.
            Some((_, current)) => Operation::Abort(current),
            // Insert the initial value.
            None => Operation::Insert((f.take().unwrap())()),
        };

        match self.compute(key, compute, guard) {
            Compute::Aborted(value) => value,
            Compute::Inserted(_, value) => value,
            _ => unreachable!(),
        }
    }

    // Updates an existing entry atomically, returning the value that was inserted.
    #[inline]
    pub fn update<'g, F>(&mut self, key: K, mut update: F, guard: &'g impl Guard) -> Option<&'g V>
    where
        F: FnMut(&V) -> V,
        K: 'g,
    {
        let compute = |entry| match entry {
            Some((_, value)) => Operation::Insert(update(value)),
            None => Operation::Abort(()),
        };

        match self.compute(key, compute, guard) {
            Compute::Updated {
                new: (_, value), ..
            } => Some(value),
            Compute::Aborted(_) => None,
            _ => unreachable!(),
        }
    }

    /// Updates an existing entry or inserts a default value computed from a closure.
    #[inline]
    pub fn update_or_insert_with<'g, U, F>(
        &mut self,
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
        let mut f = Some(f);
        let compute = |entry| match entry {
            // Perform the update.
            Some((_, value)) => Operation::Insert::<_, ()>(update(value)),
            // Insert the initial value.
            None => Operation::Insert((f.take().unwrap())()),
        };

        match self.compute(key, compute, guard) {
            Compute::Updated {
                new: (_, value), ..
            } => value,
            Compute::Inserted(_, value) => value,
            _ => unreachable!(),
        }
    }

    // Update an entry with a CAS function.
    //
    // Note that `compute` closure is guaranteed to be called for a `None` input only once, allowing the insertion
    // of values that cannot be cloned or reconstructed.
    #[inline]
    pub fn compute<'g, F, T>(
        &mut self,
        key: K,
        compute: F,
        guard: &'g impl Guard,
    ) -> Compute<'g, K, V, T>
    where
        F: FnMut(Option<(&'g K, &'g V)>) -> Operation<V, T>,
    {
        // Lazy initialize the entry allocation.
        let mut entry = LazyEntry::Uninit(key);

        // Perform the update.
        //
        // Safety: We just allocated the entry above.
        let result =
            unsafe { self.compute_with(&mut entry, ComputeState::new(compute), true, guard) };

        // Deallocate the entry if it was not inserted.
        if matches!(result, Compute::Removed(..) | Compute::Aborted(_)) {
            if let LazyEntry::Init(entry) = entry {
                // Safety: We allocated this box above and it was not inserted into the map.
                let _ = unsafe { Box::from_raw(entry) };
            }
        }

        result
    }

    // Update an entry with a CAS function.
    //
    // # Safety
    //
    // The new entry must be a valid pointer.
    #[inline]
    unsafe fn compute_with<'g, F, T>(
        &mut self,
        new_entry: &mut LazyEntry<K, V>,
        mut state: ComputeState<F, K, V, T>,
        help_copy: bool,
        guard: &'g impl Guard,
    ) -> Compute<'g, K, V, T>
    where
        F: FnMut(Option<(&'g K, &'g V)>) -> Operation<V, T>,
    {
        // The table has not yet been allocated.
        if self.table.raw.is_null() {
            // Compute the value to insert.
            match state.next(None) {
                op @ Operation::Insert(_) => state.restore(None, op),
                Operation::Remove => panic!("Cannot remove `None` entry."),
                Operation::Abort(value) => return Compute::Aborted(value),
            }

            // Initialize the table.
            self.init(None);
        }

        // Initialize the probe state.
        let (h1, h2) = self.hash(new_entry.key());
        let mut probe = Probe::start(h1, self.table.mask);

        // Probe until we reach the limit.
        let copying = 'probe: loop {
            if probe.len > self.table.limit {
                break None;
            }

            // Load the entry metadata first for cheap searches.
            let meta = unsafe { self.table.meta(probe.i) }.load(Ordering::Acquire);

            // The entry is empty.
            let mut entry = if meta == meta::EMPTY {
                // Compute the value to insert.
                let value = match state.next(None) {
                    Operation::Insert(value) => value,
                    Operation::Remove => panic!("Cannot remove `None` entry."),
                    Operation::Abort(value) => return Compute::Aborted(value),
                };

                let new_entry = new_entry.init(&self.root.collector, guard);
                unsafe { (*new_entry).value = MaybeUninit::new(value) }

                // Attempt to insert.
                match self.insert_at(probe.i, h2, new_entry.cast(), guard) {
                    // Successfully inserted.
                    InsertStatus::Inserted => {
                        // Increment the table length.
                        let count = self.root.count.get(guard.thread().id());
                        count.fetch_add(1, Ordering::Relaxed);

                        let new = unsafe { &*new_entry.cast::<Entry<K, V>>() };
                        return Compute::Inserted(&new.key, &new.value);
                    }

                    // Lost to a concurrent insert.
                    //
                    // If the key matches, we might be able to update the value.
                    InsertStatus::Found(EntryStatus::Value(found))
                    | InsertStatus::Found(EntryStatus::Copied(found)) => {
                        // Save the previous value in case the update fails.
                        let value = unsafe { (*new_entry).value.assume_init_read() };
                        state.restore(None, Operation::Insert(value));

                        found
                    }

                    // The entry was removed or invalidated.
                    InsertStatus::Found(EntryStatus::Null) => {
                        // Save the previous value.
                        let value = unsafe { (*new_entry).value.assume_init_read() };
                        state.restore(None, Operation::Insert(value));

                        // Continue probing.
                        probe.next(self.table.mask);
                        continue 'probe;
                    }
                }
            }
            // Found a potential match.
            else if meta == h2 {
                // Load the full entry.
                let found = guard
                    .protect(self.table.entry(probe.i), Ordering::Acquire)
                    .unpack();

                // The entry was deleted, keep probing.
                if found.ptr.is_null() {
                    probe.next(self.table.mask);
                    continue 'probe;
                }

                // If the key matches, we might be able to update the value.
                found
            }
            // Otherwise, continue probing.
            else {
                probe.next(self.table.mask);
                continue 'probe;
            };

            // Check for a full match.
            if unsafe { (*entry.ptr).key != *new_entry.key() } {
                probe.next(self.table.mask);
                continue 'probe;
            }

            // The entry is being copied to the new table.
            if entry.tag() & Entry::COPYING != 0 {
                break 'probe Some(probe.i);
            }

            loop {
                // Compute the value to insert.
                let failure = match state.next(Some(entry.ptr)) {
                    // The operation was aborted.
                    Operation::Abort(value) => return Compute::Aborted(value),

                    // Update the value.
                    Operation::Insert(value) => {
                        let new_entry = new_entry.init(&self.root.collector, guard);
                        unsafe { (*new_entry).value = MaybeUninit::new(value) }

                        // Try to perform the update.
                        match self.update_at(probe.i, entry, new_entry.cast(), guard) {
                            // Successfully updated.
                            UpdateStatus::Replaced(entry) => {
                                let old = unsafe { &(*entry.ptr) };
                                let new = unsafe { &*new_entry.cast::<Entry<K, V>>() };

                                return Compute::Updated {
                                    old: (&old.key, &old.value),
                                    new: (&new.key, &new.value),
                                };
                            }

                            // The update failed.
                            failure => {
                                // Save the previous value.
                                let value = unsafe { (*new_entry).value.assume_init_read() };
                                state.restore(Some(entry.ptr), Operation::Insert(value));

                                failure
                            }
                        }
                    }
                    Operation::Remove => {
                        match unsafe { self.update_at(probe.i, entry, Entry::TOMBSTONE, guard) } {
                            // Successfully removed the entry.
                            UpdateStatus::Replaced(entry) => {
                                // Mark the entry as a tombstone.
                                //
                                // Note that this might end up being overwritten by the metadata hash
                                // if the initial insertion is lagging behind, but we avoid the RMW
                                // and sacrifice reads in the extremely rare case.
                                unsafe {
                                    self.table
                                        .meta(probe.i)
                                        .store(meta::TOMBSTONE, Ordering::Release)
                                };

                                // Decrement the table length.
                                let count = self.root.count.get(guard.thread().id());
                                count.fetch_sub(1, Ordering::Relaxed);

                                let entry = unsafe { &(*entry.ptr) };
                                return Compute::Removed(&entry.key, &entry.value);
                            }

                            // The remove failed.
                            failure => {
                                // Save the removal operation.
                                state.restore(Some(entry.ptr), Operation::Remove);

                                failure
                            }
                        }
                    }
                };

                match failure {
                    // The entry is being copied to the new table.
                    UpdateStatus::Found(EntryStatus::Copied(_)) => break 'probe Some(probe.i),

                    // The entry was deleted before we could update it.
                    //
                    // We know that at some point during our execution the key was not in the map.
                    UpdateStatus::Found(EntryStatus::Null) => {
                        // Compute the next operation.
                        match state.next(None) {
                            Operation::Insert(value) => {
                                // Save the computed value.
                                state.restore(None, Operation::Insert(value));

                                // Continue probing to find an empty slot.
                                probe.next(self.table.mask);
                                continue 'probe;
                            }
                            Operation::Remove => panic!("Cannot remove `None` entry."),
                            Operation::Abort(value) => return Compute::Aborted(value),
                        }
                    }

                    // Someone else beat us to the update, retry.
                    UpdateStatus::Found(EntryStatus::Value(found)) => entry = found,

                    _ => unreachable!(),
                }
            }
        };

        match self.root.resize {
            ResizeMode::Blocking => match copying {
                // The entry we want to update is being copied.
                Some(_) => {
                    // In blocking mode we must complete the resize before proceeding.
                    let next_table = self.help_copy(guard, false);

                    // Continue in the new table.
                    return self
                        .as_ref(next_table)
                        .compute_with(new_entry, state, help_copy, guard);
                }
                // If we went over the probe limit, the key is not in this table.
                None => {}
            },

            ResizeMode::Incremental(_) => {
                // The entry we want to update is being copied.
                if let Some(i) = copying {
                    let mut next_table = self.next_table_ref().unwrap();

                    // Help out with the copy.
                    if help_copy {
                        self.help_copy(guard, false);
                    }

                    // Wait for the entry to be copied.
                    //
                    // We could perform an update based on the copied entry and
                    // race to insert it into the table here instead. However, this
                    // entire code path is very rare and likely to complete quickly,
                    // so blocking allows us to make copies faster.
                    self.wait_copied(i);

                    // Continue in the new table.
                    return next_table.compute_with(new_entry, state, false, guard);
                }

                // In incremental resize mode, we have to check the next table if we found
                // a copied entry or went over the probe limit.
                if let Some(mut next_table) = self.next_table_ref() {
                    // Help out with the copy.
                    if help_copy {
                        self.help_copy(guard, false);
                    }

                    // Continue in the new table.
                    return next_table.compute_with(new_entry, state, false, guard);
                }
            }
        }

        // Otherwise, the key is not in the table.
        match state.next(None) {
            // Need to insert into the new table.
            op @ Operation::Insert(_) => {
                // Trigger a resize.
                self.get_or_alloc_next(None);

                // Help out with the resize.
                let next_table = self.help_copy(guard, false);

                state.restore(None, op);
                return self
                    .as_ref(next_table)
                    .compute_with(new_entry, state, false, guard);
            }
            Operation::Remove => panic!("Cannot remove `None` entry."),
            Operation::Abort(value) => return Compute::Aborted(value),
        }
    }
}

// Resize operations.
impl<'root, K, V, S> HashMapRef<'root, K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    // Returns a reference to the given table.
    #[inline]
    fn as_ref(&self, table: Table<K, V>) -> HashMapRef<'root, K, V, S> {
        HashMapRef {
            table,
            root: self.root,
        }
    }

    // Returns a reference to the next table, if it has already been created.
    #[inline]
    fn next_table_ref(&self) -> Option<HashMapRef<'root, K, V, S>> {
        let state = self.table.state();
        let next = state.next.load(Ordering::Acquire);

        if !next.is_null() {
            return unsafe { Some(self.as_ref(Table::from_raw(next))) };
        }

        None
    }

    // Allocate the initial table.
    #[cold]
    fn init(&mut self, capacity: Option<usize>) -> bool {
        const CAPACITY: usize = 32;

        // Allocate the table and mark it as the root.
        let mut table = Table::<K, V>::alloc(capacity.unwrap_or(CAPACITY), &self.root.collector);
        *table.state_mut().status.get_mut() = State::PROMOTED;

        // Race to write the initial table.
        match self.root.table.compare_exchange(
            ptr::null_mut(),
            table.raw,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            // Successfully initialized the table.
            Ok(_) => {
                self.table = table;
                true
            }

            // Someone beat us, deallocate our table and use the table that was written.
            Err(found) => {
                unsafe { Table::dealloc(table) }
                self.table = unsafe { Table::from_raw(found) };
                false
            }
        }
    }

    // Returns the next table, allocating it has not already been created.
    #[cold]
    fn get_or_alloc_next(&self, capacity: Option<usize>) -> Table<K, V> {
        // Avoid spinning in tests, which can hide race conditions.
        const SPIN_ALLOC: usize = if cfg!(any(test, debug_assertions)) {
            1
        } else {
            7
        };

        let state = self.table.state();
        let next = state.next.load(Ordering::Acquire);

        // The next table is already allocated.
        if !next.is_null() {
            return unsafe { Table::from_raw(next) };
        }

        // Otherwise, try to acquire the allocation lock.
        //
        // Unlike in `init`, we do not race here to prevent unnecessary allocator pressure.
        let _allocating = match state.allocating.try_lock() {
            Ok(lock) => lock,
            // Someone else is currently allocating.
            Err(_) => {
                let mut spun = 0;

                // Spin for a bit, waiting for the table to be initialized.
                while spun <= SPIN_ALLOC {
                    for _ in 0..(spun * spun) {
                        hint::spin_loop();
                    }

                    let next = state.next.load(Ordering::Acquire);
                    if !next.is_null() {
                        // The table was initialized.
                        return unsafe { Table::from_raw(next) };
                    }

                    spun += 1;
                }

                // Otherwise, we have to block.
                state.allocating.lock().unwrap()
            }
        };

        let next = state.next.load(Ordering::Acquire);
        if !next.is_null() {
            // The table was allocated while we were waiting for the lock.
            return unsafe { Table::from_raw(next) };
        }

        let next_capacity = match cfg!(papaya_stress) {
            // Never grow the table to stress the incremental resizing algorithm.
            true => self.table.len(),
            // Double the table capacity if we are at least 50% full.
            //
            // Loading the length here is quite expensive, we may want to consider
            // a probabilistic counter to detect high-deletion workloads.
            false if self.root.len() >= (self.table.len() >> 1) => self.table.len() << 1,
            // Otherwise keep the capacity the same.
            //
            // This can occur due to poor hash distribution or frequent cycling of
            // insertions and deletions, in which case we want to avoid continuously
            // growing the table.
            false => self.table.len(),
        };

        let next_capacity = capacity.unwrap_or(next_capacity);
        assert!(
            next_capacity <= isize::MAX as usize,
            "`HashMap` exceeded maximum capacity"
        );

        // Allocate the new table while holding the lock.
        let next = Table::alloc(next_capacity, &self.root.collector);
        state.next.store(next.raw, Ordering::Release);
        drop(_allocating);

        next
    }

    // Help along with an existing resize operation, returning the new root table.
    //
    // If `copy_all` is `false` in incremental resize mode, this returns the current reference's next table,
    // not necessarily the new root.
    #[cold]
    fn help_copy(&self, guard: &impl Guard, copy_all: bool) -> Table<K, V> {
        match self.root.resize {
            ResizeMode::Blocking => self.help_copy_blocking(guard),
            ResizeMode::Incremental(chunk) => {
                let copied_to = self.help_copy_incremental(chunk, copy_all, guard);

                if !copy_all {
                    // If we weren't trying to linearize, we have to write to the next table
                    // even if the copy hasn't completed yet.
                    return self.next_table_ref().unwrap().table;
                }

                copied_to
            }
        }
    }

    // Help along the resize operation until it completes and the next table is promoted.
    //
    // Should only be called on the root table.
    fn help_copy_blocking(&self, guard: &impl Guard) -> Table<K, V> {
        // Load the next table.
        let next = self.table.state().next.load(Ordering::Acquire);
        debug_assert!(!next.is_null());
        let mut next = unsafe { Table::<K, V>::from_raw(next) };

        'copy: loop {
            // Make sure we are copying to the correct table.
            while next.state().status.load(Ordering::Relaxed) == State::ABORTED {
                next = self.as_ref(next).get_or_alloc_next(None);
            }

            // The copy already completed
            if self.try_promote(next, 0, guard) {
                return next;
            }

            let copy_chunk = self.table.len().min(4096);

            loop {
                // Every entry has already been claimed.
                if next.state().claim.load(Ordering::Relaxed) >= self.table.len() {
                    break;
                }

                // Claim a chunk to copy.
                let copy_start = next.state().claim.fetch_add(copy_chunk, Ordering::Relaxed);

                // Copy our chunk of entries.
                let mut copied = 0;
                for i in 0..copy_chunk {
                    let i = copy_start + i;

                    if i >= self.table.len() {
                        break;
                    }

                    // Copy the entry.
                    if !self.copy_at_blocking(i, next, guard) {
                        // This table doesn't have space for the next entry.
                        //
                        // Abort the current resize.
                        next.state().status.store(State::ABORTED, Ordering::Relaxed);

                        // Allocate the next table.
                        let allocated = self.as_ref(next).get_or_alloc_next(None);

                        // Wake anyone waiting for us to finish.
                        atomic_wait::wake_all(&next.state().status);

                        // Retry in a new table.
                        next = allocated;
                        continue 'copy;
                    }

                    copied += 1;
                }

                // Are we done?
                if self.try_promote(next, copied, guard) {
                    return next;
                }

                // If the resize was aborted while we were copying, continue in the new table.
                if next.state().status.load(Ordering::Relaxed) == State::ABORTED {
                    continue 'copy;
                }
            }

            // We copied all that we can, wait for the table to be promoted.
            for spun in 0.. {
                // Avoid spinning in tests, which can hide race conditions.
                const SPIN_WAIT: usize = if cfg!(any(test, debug_assertions)) {
                    1
                } else {
                    7
                };

                let status = next.state().status.load(Ordering::Relaxed);

                // If this copy was aborted, we have to retry in the new table.
                if status == State::ABORTED {
                    continue 'copy;
                }

                // The copy has completed.
                if status == State::PROMOTED {
                    fence(Ordering::Acquire);
                    return next;
                }

                // Copy chunks are relatively small and we expect to finish quickly,
                // so spin for a bit before resorting to parking.
                if spun <= SPIN_WAIT {
                    for _ in 0..(spun * spun) {
                        hint::spin_loop();
                    }

                    continue;
                }

                // Park until the table is promoted.
                atomic_wait::wait(&next.state().status, State::PENDING);
            }
        }
    }

    // Copy the entry at the given index to the new table.
    //
    // Returns `true` if the entry was copied into the table or `false` if the table was full.
    #[inline]
    fn copy_at_blocking(&self, i: usize, next_table: Table<K, V>, guard: &impl Guard) -> bool {
        // Mark the entry as copying.
        let entry = unsafe {
            self.table
                .entry(i)
                .fetch_or(Entry::COPYING, Ordering::AcqRel)
                .unpack()
        };

        // The entry is a tombstone.
        if entry.raw == Entry::TOMBSTONE {
            return true;
        }

        // There is nothing to copy, we're done.
        if entry.ptr.is_null() {
            // Mark as a tombstone so readers avoid having to load the entry.
            unsafe { self.table.meta(i).store(meta::TOMBSTONE, Ordering::Release) };
            return true;
        }

        // Copy the value to the new table.
        unsafe {
            self.as_ref(next_table)
                .insert_copy(entry.ptr.unpack(), false, guard)
                .is_some()
        }
    }

    // Help along an in-progress resize incrementally by copying a chunk of entries.
    //
    // Returns the table that was copied to.
    fn help_copy_incremental(&self, chunk: usize, block: bool, guard: &impl Guard) -> Table<K, V> {
        // Always help the highest priority root resize.
        let root = self.root.root(guard);
        if self.table.raw != root.table.raw {
            return root.help_copy_incremental(chunk, block, guard);
        }

        // Load the next table.
        let next = self.table.state().next.load(Ordering::Acquire);

        // The copy we tried to help was already promoted.
        if next.is_null() {
            return self.table;
        }

        let next = unsafe { Table::<K, V>::from_raw(next) };

        loop {
            // The copy already completed.
            if self.try_promote(next, 0, guard) {
                return next;
            }

            loop {
                // Every entry has already been claimed.
                if next.state().claim.load(Ordering::Relaxed) >= self.table.len() {
                    break;
                }

                // Claim a chunk to copy.
                let copy_start = next.state().claim.fetch_add(chunk, Ordering::Relaxed);

                // Copy our chunk of entries.
                let mut copied = 0;
                for i in 0..chunk {
                    let i = copy_start + i;

                    if i >= self.table.len() {
                        break;
                    }

                    // Copy the entry.
                    self.copy_at_incremental(i, next, guard);
                    copied += 1;
                }

                // Update the copy state, and try to promote the table.
                //
                // Only copy a single chunk if promotion fails, unless we are forced
                // to complete the resize.
                if self.try_promote(next, copied, guard) || !block {
                    return next;
                }
            }

            // There are no entries that we can copy, block if necessary.
            if !block {
                return next;
            }

            for spun in 0.. {
                // Avoid spinning in tests, which can hide race conditions.
                const SPIN_WAIT: usize = if cfg!(any(test, debug_assertions)) {
                    1
                } else {
                    7
                };

                // The copy has completed.
                let status = next.state().status.load(Ordering::Acquire);
                if status == State::PROMOTED {
                    return next;
                }

                // Copy chunks are relatively small and we expect to finish quickly,
                // so spin for a bit before resorting to parking.
                if spun <= SPIN_WAIT {
                    for _ in 0..(spun * spun) {
                        hint::spin_loop();
                    }

                    continue;
                }

                // Park until the table is promoted.
                atomic_wait::wait(&next.state().status, State::PENDING);
            }
        }
    }

    // Copy the entry at the given index to the new table.
    #[inline]
    fn copy_at_incremental(&self, i: usize, next_table: Table<K, V>, guard: &impl Guard) {
        // Mark the entry as copying.
        let entry = unsafe {
            self.table
                .entry(i)
                .fetch_or(Entry::COPYING, Ordering::AcqRel)
                .unpack()
        };

        // The entry is a tombstone.
        if entry.raw == Entry::TOMBSTONE {
            return;
        }

        // There is nothing to copy, we're done.
        if entry.ptr.is_null() {
            // Mark as a tombstone so readers avoid having to load the entry.
            unsafe { self.table.meta(i).store(meta::TOMBSTONE, Ordering::Release) };
            return;
        }

        // Mark the entry as borrowed so writers in the new table know it was copied.
        let new_entry = entry.map_tag(|addr| addr | Entry::BORROWED);

        // Copy the value to the new table.
        unsafe {
            self.as_ref(next_table)
                .insert_copy(new_entry, true, guard)
                .unwrap();
        }

        // Mark the entry as copied.
        let copied = entry
            .raw
            .map_addr(|addr| addr | Entry::COPYING | Entry::COPIED);

        // Note that we already wrote the COPYING bit, so no one is writing to the old
        // entry except us.
        unsafe { self.table.entry(i).store(copied, Ordering::SeqCst) };

        // Notify any writers that the copy has completed.
        unsafe { self.table.state().parker.unpark(self.table.entry(i)) };
    }

    // Copy an entry into the table, returning the index it was inserted into.
    //
    // This is an optimized version of `insert_entry` where the caller is the only writer
    // inserting the given key into the new table, as it has already been marked as copying.
    #[inline]
    unsafe fn insert_copy(
        &self,
        new_entry: Tagged<Entry<K, V>>,
        resize: bool,
        guard: &impl Guard,
    ) -> Option<(Table<K, V>, usize)> {
        // Safety: The new entry is guaranteed to be valid by the caller.
        let key = unsafe { &(*new_entry.ptr).key };

        // Initialize the probe state.
        let (h1, h2) = self.hash(key);
        let mut probe = Probe::start(h1, self.table.mask);

        // Probe until we reach the limit.
        while probe.len <= self.table.limit {
            // Load the entry metadata first for cheap searches.
            let meta = unsafe { self.table.meta(probe.i) }.load(Ordering::Acquire);

            // The entry is empty, try to insert.
            if meta == meta::EMPTY {
                let entry = unsafe { self.table.entry(probe.i) };

                // Try to claim the entry.
                match entry.compare_exchange(
                    ptr::null_mut(),
                    new_entry.raw,
                    Ordering::Release,
                    Ordering::Acquire,
                ) {
                    // Successfully inserted.
                    Ok(_) => {
                        // Update the metadata table.
                        unsafe { self.table.meta(probe.i).store(h2, Ordering::Release) };
                        return Some((self.table, probe.i));
                    }
                    Err(found) => {
                        // The entry was deleted or copied.
                        let meta = if found.unpack().ptr.is_null() {
                            meta::TOMBSTONE
                        } else {
                            // Protect the entry before accessing it.
                            let found = guard.protect(entry, Ordering::Acquire).unpack();

                            // Recheck the pointer.
                            if found.ptr.is_null() {
                                meta::TOMBSTONE
                            } else {
                                // Ensure the meta table is updated to avoid breaking the probe chain.
                                let hash = self.root.hasher.hash_one(&(*found.ptr).key);
                                meta::h2(hash)
                            }
                        };

                        if self.table.meta(probe.i).load(Ordering::Relaxed) == meta::EMPTY {
                            self.table.meta(probe.i).store(meta, Ordering::Release);
                        }
                    }
                }
            }

            // Continue probing.
            probe.next(self.table.mask);
        }

        if !resize {
            return None;
        }

        // Insert into the next table.
        let next_table = self.get_or_alloc_next(None);
        self.as_ref(next_table)
            .insert_copy(new_entry, resize, guard)
    }

    // Update the copy state and attempt to promote a table to the root.
    //
    // Returns `true` if the table was promoted.
    #[inline]
    fn try_promote(&self, next: Table<K, V>, copied: usize, guard: &impl Guard) -> bool {
        // Update the copy count.
        let copied = if copied > 0 {
            next.state().copied.fetch_add(copied, Ordering::AcqRel) + copied
        } else {
            next.state().copied.load(Ordering::Acquire)
        };

        // If we copied all the entries in the table, we can try to promote.
        if copied == self.table.len() {
            let root = self.root.table.load(Ordering::Relaxed);

            // Only promote root copies.
            //
            // We can't promote a nested copy before it's parent has finished, as
            // it may not contain all the entries in the table.
            if self.table.raw == root {
                // Try to update the root.
                if self
                    .root
                    .table
                    .compare_exchange(
                        self.table.raw,
                        next.raw,
                        Ordering::Release,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    // Successfully promoted the table.
                    next.state()
                        .status
                        .store(State::PROMOTED, Ordering::Release);

                    unsafe {
                        // Retire the old table.
                        //
                        // Note that we do not drop entries because they have been copied to the
                        // new root.
                        guard.defer_retire(self.table.raw, |link| {
                            let raw: *mut RawTable = link.cast();
                            let table = Table::<K, V>::from_raw(raw);
                            drop_table::<K, V>(table);
                        });
                    }
                }

                // Wake up any writers waiting for the resize to complete.
                atomic_wait::wake_all(&next.state().status);
                return true;
            }
        }

        // Not ready to promote yet.
        false
    }

    // Completes all pending copies in incremental mode to get a clean copy of the table.
    //
    // This is necessary for operations like `iter` or `clear`, where entries in multiple tables
    // can cause lead to incomplete results.
    #[inline]
    fn linearize(&mut self, guard: &impl Guard) {
        if self.root.is_incremental() {
            // If we're in incremental resize mode, we need to complete any in-progress resizes to
            // ensure we don't miss any entries in the next table. We can't iterate over both because
            // we risk returning the same entry twice.
            while self.next_table_ref().is_some() {
                self.table = self.help_copy(guard, true);
            }
        }
    }

    // Wait for an incremental copy of a given entry to complete.
    #[inline]
    fn wait_copied(&self, i: usize) {
        // Avoid spinning in tests, which can hide race conditions.
        const SPIN_WAIT: usize = if cfg!(any(test, debug_assertions)) {
            1
        } else {
            5
        };

        let parker = &self.table.state().parker;
        let entry = unsafe { self.table.entry(i) };

        // Spin for a short while, waiting for the entry to be copied.
        for spun in 0..SPIN_WAIT {
            for _ in 0..(spun * spun) {
                hint::spin_loop();
            }

            // The entry was copied.
            let entry = entry.load(Ordering::Acquire).unpack();
            if entry.tag() & Entry::COPIED != 0 {
                return;
            }
        }

        // Park until the copy completes.
        parker.park(entry, |entry| entry.addr() & Entry::COPIED == 0);
    }

    /// Retire an entry that was removed from the current table, but may still be reachable from
    /// previous tables.
    ///
    /// # Safety
    ///
    /// The entry must be unreachable from the current table.
    #[inline]
    unsafe fn defer_retire(&self, entry: Tagged<Entry<K, V>>, guard: &impl Guard) {
        match self.root.resize {
            // Safety: In blocking resize mode, we only ever write to the root table, so the entry
            // is inaccessible from all tables.
            ResizeMode::Blocking => unsafe {
                guard.defer_retire(entry.ptr, Entry::reclaim::<K, V>);
            },
            // In incremental resize mode, the entry may be accessible in previous tables.
            ResizeMode::Incremental(_) => {
                if entry.tag() & Entry::BORROWED == 0 {
                    // Safety: If the entry is not borrowed, meaning it is not in any previous tables,
                    // it is inaccessible even if we are not the root. Thus we can safely retire.
                    unsafe { guard.defer_retire(entry.ptr, Entry::reclaim::<K, V>) };
                    return;
                }

                let root = self.root.table.load(Ordering::Relaxed);

                // Check if our table, or any subsequent table, is the root.
                let mut next = Some(self.clone());
                while let Some(map) = next {
                    if map.table.raw == root {
                        // Safety: The root table is our table or a table that succeeds ours.
                        // Thus any previous tables are unreachable and we can safely retire.
                        unsafe { guard.defer_retire(entry.ptr, Entry::reclaim::<K, V>) };
                        return;
                    }

                    next = map.next_table_ref();
                }

                // Otherwise, we have to wait for the table we are copying from to be reclaimed.
                //
                // Find the table we are copying from, searching from the root.
                fence(Ordering::Acquire);
                let mut prev = self.as_ref(unsafe { Table::<K, V>::from_raw(root) });

                loop {
                    let next = prev.next_table_ref().unwrap();

                    // Defer the entry to be retired by the table we are copying from.
                    if next.table.raw == self.table.raw {
                        prev.table.state().deferred.defer(entry.ptr);
                        return;
                    }

                    prev = next;
                }
            }
        }
    }
}

// An iterator over the keys and values of this table.
pub struct Iter<'g, K, V, G> {
    i: usize,
    table: Table<K, V>,
    guard: &'g G,
}

impl<'g, K: 'g, V: 'g, G> Iterator for Iter<'g, K, V, G>
where
    G: Guard,
{
    type Item = (&'g K, &'g V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // The table has not yet been allocated.
        if self.table.raw.is_null() {
            return None;
        }

        loop {
            // Iterated over every entry in the table, we're done.
            if self.i >= self.table.len() {
                return None;
            }

            // Load the entry metadata first to ensure consistency with calls to `get`.
            let meta = unsafe { self.table.meta(self.i) }.load(Ordering::Acquire);

            // The entry is empty or deleted.
            if matches!(meta, meta::EMPTY | meta::TOMBSTONE) {
                self.i += 1;
                continue;
            }

            // Load the entry.
            let entry = unsafe {
                self.guard
                    .protect(self.table.entry(self.i), Ordering::Acquire)
                    .unpack()
            };

            // The entry was deleted.
            if entry.ptr.is_null() {
                self.i += 1;
                continue;
            }

            // Read the key and value.
            let entry = unsafe { (&(*entry.ptr).key, &(*entry.ptr).value) };

            self.i += 1;
            return Some(entry);
        }
    }
}

// Safety: An iterator holds a shared reference to the HashMap
// and Guard, and outputs shared references to keys and values.
// Thus everything must be Sync for the iterator to be Send/Sync.
unsafe impl<K, V, G> Send for Iter<'_, K, V, G>
where
    K: Sync,
    V: Sync,
    G: Sync,
{
}

unsafe impl<K, V, G> Sync for Iter<'_, K, V, G>
where
    K: Sync,
    V: Sync,
    G: Sync,
{
}

impl<K, V, G> Clone for Iter<'_, K, V, G> {
    #[inline]
    fn clone(&self) -> Self {
        Iter {
            i: self.i,
            table: self.table,
            guard: self.guard,
        }
    }
}

impl<K, V, S> Clone for HashMapRef<'_, K, V, S> {
    #[inline]
    fn clone(&self) -> Self {
        HashMapRef {
            table: self.table,
            root: self.root,
        }
    }
}

impl<K, V, S> Drop for HashMap<K, V, S> {
    fn drop(&mut self) {
        let mut raw = *self.table.get_mut();

        // Make sure all objects are reclaimed before the collector is dropped.
        //
        // Dropping a table depends on accessing the collector for deferred retirement,
        // using the shared collector pointer that is invalidated by drop.
        unsafe { self.collector.reclaim_all() };

        // Drop all nested tables and entries.
        while !raw.is_null() {
            let mut table = unsafe { Table::<K, V>::from_raw(raw) };
            let next = *table.state_mut().next.get_mut();
            unsafe { drop_entries::<K, V>(table) };
            unsafe { drop_table::<K, V>(table) };
            raw = next;
        }
    }
}

// Drop all entries in this table.
unsafe fn drop_entries<K, V>(table: Table<K, V>) {
    for i in 0..table.len() {
        let entry = unsafe { (*table.entry(i).as_ptr()).unpack() };

        // The entry was copied, or there is nothing to deallocate.
        if entry.ptr.is_null() || entry.tag() & Entry::COPYING != 0 {
            continue;
        }

        // Drop the entry.
        unsafe { Entry::reclaim::<K, V>(entry.ptr.cast()) }
    }
}

// Drop the table allocation.
unsafe fn drop_table<K, V>(mut table: Table<K, V>) {
    // Safety: `drop_table` is being called from `reclaim_all` in `Drop` or
    // a table is being reclaimed by our thread. In both cases, the collector
    // is still alive and safe to access through the state pointer.
    let collector = unsafe { &*table.state().collector };

    // Drop any entries that were deferred during an incremental resize.
    //
    // Safety: A deferred entry was retired after it was made unreachable
    // from the next table during a resize. Because our table was still accessible
    // for this entry to be deferred, our table must have been retired *after* the
    // entry was made accessible in the next table. Now that our table is being reclaimed,
    // the entry has thus been totally removed from the map, and can be safely retired.
    unsafe {
        table
            .state_mut()
            .deferred
            .retire_all(collector, Entry::reclaim::<K, V>)
    }

    // Deallocate the table.
    unsafe { Table::dealloc(table) };
}

// Entry metadata, inspired by `hashbrown`.
mod meta {
    use std::mem;

    // Indicates an empty entry.
    pub const EMPTY: u8 = 0x80;

    // Indicates an entry that has been deleted.
    pub const TOMBSTONE: u8 = u8::MAX;

    // Returns the primary hash for an entry.
    #[inline]
    pub fn h1(hash: u64) -> usize {
        hash as usize
    }

    /// Return a byte of hash metadata, used for cheap searches.
    #[inline]
    pub fn h2(hash: u64) -> u8 {
        const MIN_HASH_LEN: usize = if mem::size_of::<usize>() < mem::size_of::<u64>() {
            mem::size_of::<usize>()
        } else {
            mem::size_of::<u64>()
        };

        // Grab the top 7 bits of the hash.
        //
        // While the hash is normally a full 64-bit value, some hash functions
        // (such as fxhash) produce a usize result instead, which means that the
        // top 32 bits are 0 on 32-bit platforms.
        let top7 = hash >> (MIN_HASH_LEN * 8 - 7);
        (top7 & 0x7f) as u8
    }
}
