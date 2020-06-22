use crate::alloc::{sarc_add_copy, sarc_deref, sarc_remove_copy, ABox};
use std::ops::Deref;

pub struct Element<K, V> {
    pub hash: u64,
    pub key: K,
    pub value: V,
}

impl<K, V> Element<K, V> {
    pub fn new(key: K, hash: u64, value: V) -> Self {
        Self { hash, key, value }
    }

    pub fn read(ptr: *mut ABox<Element<K, V>>) -> ElementGuard<K, V> {
        sarc_add_copy(ptr);

        let data = sarc_deref(ptr);
        let key = &data.key;
        let value = &data.value;

        ElementGuard {
            mem_guard: ptr,
            key,
            value,
        }
    }
}

/// `ElementGuard<K, V>`'s are references to active or past map entries.
/// They provide access to the key and value. They exist to automatically manage memory
/// across threads to ensure a safe interface.
pub struct ElementGuard<K, V> {
    pub(crate) mem_guard: *mut ABox<Element<K, V>>,
    key: *const K,
    value: *const V,
}

impl<K, V> Clone for ElementGuard<K, V> {
    fn clone(&self) -> Self {
        sarc_add_copy(self.mem_guard);

        Self {
            mem_guard: self.mem_guard,
            key: self.key,
            value: self.value,
        }
    }
}

impl<K, V> Drop for ElementGuard<K, V> {
    fn drop(&mut self) {
        sarc_remove_copy(self.mem_guard);
    }
}

impl<K, V> ElementGuard<K, V> {
    /// Get references to the key and value.
    pub fn pair(&self) -> (&K, &V) {
        // # Safety
        // This is okay to do becaues the references created here may
        // not outlive the guard and thus always point to valid data.
        unsafe { (&*self.key, &*self.value) }
    }

    /// Get a reference to the key.
    pub fn key(&self) -> &K {
        self.pair().0
    }

    /// Get a reference to the value.
    pub fn value(&self) -> &V {
        self.pair().1
    }
}

impl<K, V> Deref for ElementGuard<K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

// # Safety
// This is okay since we are not keeping any state that is not unsafe to share across threads.
// We are just working around the fact that pointers are not `Send` nor `Sync`.
unsafe impl<K: Send, V: Send> Send for ElementGuard<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for ElementGuard<K, V> {}
