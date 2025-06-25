use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use core::hash::Hash;
use core::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct RefMulti<'a, K, V> {
    _guard: Arc<RwLockReadGuardDetached<'a>>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> RefMulti<'a, K, V> {
    pub(crate) fn new(guard: Arc<RwLockReadGuardDetached<'a>>, k: &'a K, v: &'a V) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &'a K {
        self.pair().0
    }

    pub fn value(&self) -> &'a V {
        self.pair().1
    }

    pub fn pair(&self) -> (&'a K, &'a V) {
        (self.k, self.v)
    }
}

impl<'a, K: Eq + Hash, V> Deref for RefMulti<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &'a V {
        self.value()
    }
}

pub struct RefMutMulti<'a, K, V> {
    _guard: Arc<RwLockWriteGuardDetached<'a>>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> RefMutMulti<'a, K, V> {
    pub(crate) fn new(guard: Arc<RwLockWriteGuardDetached<'a>>, k: &'a K, v: &'a mut V) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn value_mut(&mut self) -> &mut V {
        self.pair_mut().1
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        (self.k, self.v)
    }
}

impl<'a, K: Eq + Hash, V> Deref for RefMutMulti<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for RefMutMulti<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::DashMap;

    #[test]
    #[allow(unused)]
    fn lifetime() {
        fn get_key<'a>(data: &'a DashMap<String, String>) -> impl Iterator<Item = &'a str> {
            data.iter().map(|item| item.key().as_str())
        }
        fn get_value<'a>(data: &'a DashMap<String, String>) -> impl Iterator<Item = &'a str> {
            data.iter().map(|item| item.value().as_str())
        }
        fn get_pair(data: &DashMap<String, String>) -> impl Iterator<Item = (&String, &String)> {
            data.iter().map(|item| item.pair())
        }
    }
}
