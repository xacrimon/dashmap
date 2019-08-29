use hashbrown::HashMap;
use owning_ref::{OwningRef, OwningRefMut};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::ops::{Deref, DerefMut};
use std::hash::Hash;

// -- Shared

pub struct DashMapRef<'a, K: Eq + Hash, V> {
    ptr: OwningRef<RwLockReadGuard<'a, HashMap<K, V>>, V>,
}

impl<'a, K: Eq + Hash, V> DashMapRef<'a, K, V> {
    pub(crate) fn new(ptr: OwningRef<RwLockReadGuard<'a, HashMap<K, V>>, V>) -> Self {
        Self {
            ptr,
        }
    }

    pub fn value(&self) -> &V {
        &*self.ptr
    }
}

impl<'a, K: Eq + Hash, V> Deref for DashMapRef<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

// --

// -- Unique

pub struct DashMapRefMut<'a, K: Eq + Hash, V> {
    ptr: OwningRefMut<RwLockWriteGuard<'a, HashMap<K, V>>, V>,
}

impl<'a, K: Eq + Hash, V> DashMapRefMut<'a, K, V> {
    pub(crate) fn new(ptr: OwningRefMut<RwLockWriteGuard<'a, HashMap<K, V>>, V>) -> Self {
        Self {
            ptr,
        }
    }

    pub fn value(&self) -> &V {
        &*self.ptr
    }

    pub fn value_mut(&mut self) -> &mut V {
        &mut *self.ptr
    }
}

impl<'a, K: Eq + Hash, V> Deref for DashMapRefMut<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for DashMapRefMut<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

// --
