use std::hash::{BuildHasher, Hash, Hasher};
use crate::element::{Element, ElementGuard};
use crate::alloc::ABox;
use std::sync::{Arc, Mutex};

type HashMap<K, V, S> = std::collections::HashMap<K, *mut ABox<V>, S>;

pub struct Table<K, V, S> {
    inner: Mutex<HashMap<K, V, S>>,
}

impl<K: Eq + Hash, V, S: BuildHasher> Table<K, V, S> {
    pub fn new(capacity: usize, hasher: S) -> Self {
        Self {
            inner: Mutex::new(HashMap::with_capacity_and_hasher(capacity, hasher)),
        }
    }

    pub fn insert_and_get(key: K, value: V) -> Option<ElementGuard<K, V>> {

    }
}
