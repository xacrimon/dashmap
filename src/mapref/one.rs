use lock_api::RawRwLockDowngrade;

use crate::{GuardRead, GuardWrite, SharedValue};
use core::hash::Hash;
use core::ops::{Deref, DerefMut};
use std::fmt::{Debug, Formatter};
use std::mem::ManuallyDrop;
use std::ptr::addr_of;

pub struct Ref<'a, K, V> {
    guard: GuardRead<'a>,
    data: &'a (K, SharedValue<V>),
}

unsafe impl<'a, K: Eq + Hash + Sync, V: Sync> Send for Ref<'a, K, V> {}
unsafe impl<'a, K: Eq + Hash + Sync, V: Sync> Sync for Ref<'a, K, V> {}

impl<'a, K: Eq + Hash, V> Ref<'a, K, V> {
    pub(crate) unsafe fn new(guard: GuardRead<'a>, data: &'a (K, SharedValue<V>)) -> Self {
        Self { guard, data }
    }

    pub fn key(&self) -> &K {
        &self.data.0
    }

    pub fn value(&self) -> &V {
        self.data.1.get()
    }

    pub fn pair(&self) -> (&K, &V) {
        (&self.data.0, self.data.1.get())
    }

    pub fn map<F, T>(self, f: F) -> MappedRef<'a, K, T>
    where
        F: FnOnce(&V) -> &T,
    {
        MappedRef {
            _guard: self.guard,
            k: &self.data.0,
            v: f(self.data.1.get()),
        }
    }

    pub fn try_map<F, T>(self, f: F) -> Result<MappedRef<'a, K, T>, Self>
    where
        F: FnOnce(&V) -> Option<&T>,
    {
        if let Some(v) = f(self.data.1.get()) {
            Ok(MappedRef {
                _guard: self.guard,
                k: &self.data.0,
                v,
            })
        } else {
            Err(self)
        }
    }
}

impl<'a, K: Eq + Hash + Debug, V: Debug> Debug for Ref<'a, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref")
            .field("k", &self.data.0)
            .field("v", self.data.1.get())
            .finish()
    }
}

impl<'a, K: Eq + Hash, V> Deref for Ref<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMut<'a, K, V> {
    guard: GuardWrite<'a>,
    data: &'a mut (K, SharedValue<V>),
}

unsafe impl<'a, K: Eq + Hash + Sync, V: Sync> Send for RefMut<'a, K, V> {}
unsafe impl<'a, K: Eq + Hash + Sync, V: Sync> Sync for RefMut<'a, K, V> {}

impl<'a, K: Eq + Hash, V> RefMut<'a, K, V> {
    pub(crate) unsafe fn new(guard: GuardWrite<'a>, data: &'a mut (K, SharedValue<V>)) -> Self {
        Self { guard, data }
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
        (&self.data.0, self.data.1.get())
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        (&self.data.0, self.data.1.get_mut())
    }

    pub fn downgrade(self) -> Ref<'a, K, V> {
        unsafe {
            let guard = ManuallyDrop::new(self.guard);
            guard.0.downgrade();
            Ref::new(GuardRead(guard.0), &*addr_of!(*self.data))
        }
    }

    pub fn map<F, T>(self, f: F) -> MappedRefMut<'a, K, T>
    where
        F: FnOnce(&mut V) -> &mut T,
    {
        MappedRefMut {
            _guard: self.guard,
            k: &self.data.0,
            v: f(self.data.1.get_mut()),
        }
    }

    pub fn try_map<F, T>(self, f: F) -> Result<MappedRefMut<'a, K, T>, Self>
    where
        F: FnOnce(&mut V) -> Option<&mut T>,
    {
        let v = match f(unsafe { &mut *(self.data.1.get_mut() as *mut _) }) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self.guard;
        let k = &self.data.0;
        Ok(MappedRefMut {
            _guard: guard,
            k,
            v,
        })
    }
}

impl<'a, K: Eq + Hash + Debug, V: Debug> Debug for RefMut<'a, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefMut")
            .field("k", &self.data.0)
            .field("v", self.data.1.get())
            .finish()
    }
}

impl<'a, K: Eq + Hash, V> Deref for RefMut<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Eq + Hash, V> DerefMut for RefMut<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

pub struct MappedRef<'a, K, T> {
    _guard: GuardRead<'a>,
    k: &'a K,
    v: &'a T,
}

impl<'a, K: Eq + Hash, T> MappedRef<'a, K, T> {
    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &T {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &T) {
        (self.k, self.v)
    }

    pub fn map<F, T2>(self, f: F) -> MappedRef<'a, K, T2>
    where
        F: FnOnce(&T) -> &T2,
    {
        MappedRef {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRef<'a, K, T2>, Self>
    where
        F: FnOnce(&T) -> Option<&T2>,
    {
        let v = match f(self.v) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self._guard;
        Ok(MappedRef {
            _guard: guard,
            k: self.k,
            v,
        })
    }
}

impl<'a, K: Eq + Hash + Debug, T: Debug> Debug for MappedRef<'a, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRef")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Eq + Hash, T> Deref for MappedRef<'a, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<'a, K: Eq + Hash, T: std::fmt::Display> std::fmt::Display for MappedRef<'a, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.value(), f)
    }
}

impl<'a, K: Eq + Hash, T: AsRef<TDeref>, TDeref: ?Sized> AsRef<TDeref> for MappedRef<'a, K, T> {
    fn as_ref(&self) -> &TDeref {
        self.value().as_ref()
    }
}

pub struct MappedRefMut<'a, K, T> {
    _guard: GuardWrite<'a>,
    k: &'a K,
    v: &'a mut T,
}

impl<'a, K: Eq + Hash, T> MappedRefMut<'a, K, T> {
    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &T {
        self.pair().1
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.pair_mut().1
    }

    pub fn pair(&self) -> (&K, &T) {
        (self.k, &*self.v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut T) {
        (self.k, self.v)
    }

    pub fn map<F, T2>(self, f: F) -> MappedRefMut<'a, K, T2>
    where
        F: FnOnce(&mut T) -> &mut T2,
    {
        MappedRefMut {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRefMut<'a, K, T2>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut T2>,
    {
        let v = match f(unsafe { &mut *(self.v as *mut _) }) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self._guard;
        let k = self.k;
        Ok(MappedRefMut {
            _guard: guard,
            k,
            v,
        })
    }
}

impl<'a, K: Eq + Hash + Debug, T: Debug> Debug for MappedRefMut<'a, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRefMut")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Eq + Hash, T> Deref for MappedRefMut<'a, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<'a, K: Eq + Hash, T> DerefMut for MappedRefMut<'a, K, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}
