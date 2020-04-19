use crate::DashMap;
use std::hash::{Hash, BuildHasher};
use crate::ElementGuard;

pub struct VacantEntry<'a, K, V, S> {
    map: &'a DashMap<K, V, S>,
    key: K,
}

impl<'a, K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> VacantEntry<'a, K, V, S> {
    pub fn insert(self, value: V) -> ElementGuard<K, V> {
        self.map.insert_and_get(self.key, value)
    }

    pub fn into_key(self) -> K {
        self.key
    }

    pub fn key(&self) -> &K {
        &self.key
    }
}

pub struct OccupiedEntry<'a, K, V, S> {
    map: &'a DashMap<K, V, S>,
    local_key: Option<K>,
    elem: ElementGuard<K, V>,
}

impl<'a, K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> OccupiedEntry<'a, K, V, S> {
    pub fn element(&self) -> ElementGuard<K, V> {
        self.elem.clone()
    }

    pub fn insert(mut self, value: V) -> ElementGuard<K, V> {
        self.map.insert_and_get(self.local_key.take().unwrap(), value)
    }

    pub fn remove(self) -> ElementGuard<K, V> {
        let guard = self.element();
        self.map.remove(guard.key());
        guard
    }

    pub fn into_key(mut self) -> K {
        self.local_key.take().unwrap()
    }
}

pub enum Entry<'a, K, V, S> {
    Vacant(VacantEntry<'a, K, V, S>),
    Occupied(OccupiedEntry<'a, K, V, S>),
}

impl<'a, K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> Entry<'a, K, V, S> {
    pub(crate) fn new(map: &'a DashMap<K, V, S>, key: K) -> Self {
        match map.get(&key) {
            Some(elem) =>  {
                Self::Occupied(OccupiedEntry {
                    map,
                    local_key: Some(key),
                    elem,
                })
            }

            None => {
                Self::Vacant(VacantEntry { map, key })
            }
        }
    }
}
