use crate::alloc::{sarc_add_copy, sarc_deref, sarc_remove_copy, ABox};
use std::mem::transmute;
use std::ops::Deref;

#[derive(Clone)]
pub struct Element<K, V> {
    pub hash: u64,
    pub key: K,
    pub value: V,
}

impl<K, V> Element<K, V> {
    pub fn new(key: K, hash: u64, value: V) -> Self {
        Self { hash, key, value }
    }

    pub fn destructure(self) -> (K, u64, V) {
        (self.key, self.hash, self.value)
    }

    pub fn read(ptr: *mut ABox<Element<K, V>>) -> ElementGuard<K, V> {
        unsafe {
            sarc_add_copy(ptr);
            let data = sarc_deref(ptr);
            let key = transmute(&data.key);
            let value = transmute(&data.value);
            ElementGuard {
                mem_guard: ptr,
                key,
                value,
            }
        }
    }
}

pub struct ElementGuard<K, V> {
    mem_guard: *mut ABox<Element<K, V>>,
    key: *const K,
    value: *const V,
}

impl<K, V> Drop for ElementGuard<K, V> {
    fn drop(&mut self) {
        sarc_remove_copy(self.mem_guard);
    }
}

impl<K, V> ElementGuard<K, V> {
    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&*self.key, &*self.value) }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

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
