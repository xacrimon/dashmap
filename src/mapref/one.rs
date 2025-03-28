use crate::{
    lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached},
    mapref::multiple::{RefMulti, RefMutMulti},
};
use core::hash::Hash;
use core::ops::{Deref, DerefMut};
use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

pub struct Ref<'a, K, V> {
    _guard: RwLockReadGuardDetached<'a>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K: Eq + Hash, V> Ref<'a, K, V> {
    pub(crate) fn new(guard: RwLockReadGuardDetached<'a>, k: &'a K, v: &'a V) -> Self {
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

    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }

    pub fn map<F, T: ?Sized>(self, f: F) -> MappedRef<'a, K, T>
    where
        F: FnOnce(&V) -> &T,
    {
        MappedRef {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T: ?Sized>(self, f: F) -> Result<MappedRef<'a, K, T>, Self>
    where
        F: FnOnce(&V) -> Option<&T>,
    {
        if let Some(v) = f(self.v) {
            Ok(MappedRef {
                _guard: self._guard,
                k: self.k,
                v,
            })
        } else {
            Err(self)
        }
    }

    pub fn map_split<F, A: ?Sized, B: ?Sized>(
        self,
        f: F,
    ) -> (RefMulti<'a, K, A>, RefMulti<'a, K, B>)
    where
        F: FnOnce(&V) -> (&A, &B),
    {
        let (a, b) = f(self.v);
        let guard = Arc::new(self._guard);
        (
            RefMulti {
                _guard: guard.clone(),
                k: self.k,
                v: a,
            },
            RefMulti {
                _guard: guard,
                k: self.k,
                v: b,
            },
        )
    }
}

impl<'a, K: Eq + Hash + Debug, V: Debug> Debug for Ref<'a, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref")
            .field("k", &self.k)
            .field("v", &self.v)
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
    guard: RwLockWriteGuardDetached<'a>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K: Eq + Hash, V> RefMut<'a, K, V> {
    pub(crate) fn new(guard: RwLockWriteGuardDetached<'a>, k: &'a K, v: &'a mut V) -> Self {
        Self { guard, k, v }
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

    pub fn downgrade(self) -> Ref<'a, K, V> {
        Ref::new(
            unsafe { RwLockWriteGuardDetached::downgrade(self.guard) },
            self.k,
            self.v,
        )
    }

    pub fn map<F, T: ?Sized>(self, f: F) -> MappedRefMut<'a, K, T>
    where
        F: FnOnce(&mut V) -> &mut T,
    {
        MappedRefMut {
            _guard: self.guard,
            k: self.k,
            v: f(&mut *self.v),
        }
    }

    pub fn try_map<F, T: ?Sized>(self, f: F) -> Result<MappedRefMut<'a, K, T>, Self>
    where
        F: FnOnce(&mut V) -> Option<&mut T>,
    {
        let v = match f(unsafe { &mut *(self.v as *mut _) }) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self.guard;
        let k = self.k;
        Ok(MappedRefMut {
            _guard: guard,
            k,
            v,
        })
    }

    pub fn map_split<F, A: ?Sized, B: ?Sized>(
        self,
        f: F,
    ) -> (RefMutMulti<'a, K, A>, RefMutMulti<'a, K, B>)
    where
        F: FnOnce(&mut V) -> (&mut A, &mut B),
    {
        let (a, b) = f(self.v);
        let guard = Arc::new(self.guard);
        (
            RefMutMulti {
                _guard: guard.clone(),
                k: self.k,
                v: a,
            },
            RefMutMulti {
                _guard: guard,
                k: self.k,
                v: b,
            },
        )
    }
}

impl<'a, K: Eq + Hash + Debug, V: Debug> Debug for RefMut<'a, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefMut")
            .field("k", &self.k)
            .field("v", &self.v)
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

pub struct MappedRef<'a, K, T: ?Sized> {
    _guard: RwLockReadGuardDetached<'a>,
    k: &'a K,
    v: &'a T,
}

impl<'a, K: Eq + Hash, T: ?Sized> MappedRef<'a, K, T> {
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

    pub fn try_map<F, T2: ?Sized>(self, f: F) -> Result<MappedRef<'a, K, T2>, Self>
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

impl<'a, K: Eq + Hash + Debug, T: Debug + ?Sized> Debug for MappedRef<'a, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRef")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Eq + Hash, T: ?Sized> Deref for MappedRef<'a, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<'a, K: Eq + Hash, T: std::fmt::Display + ?Sized> std::fmt::Display for MappedRef<'a, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.value(), f)
    }
}

impl<'a, K: Eq + Hash, T: ?Sized + AsRef<TDeref>, TDeref: ?Sized> AsRef<TDeref>
    for MappedRef<'a, K, T>
{
    fn as_ref(&self) -> &TDeref {
        self.value().as_ref()
    }
}

pub struct MappedRefMut<'a, K, T: ?Sized> {
    _guard: RwLockWriteGuardDetached<'a>,
    k: &'a K,
    v: &'a mut T,
}

impl<'a, K: Eq + Hash, T: ?Sized> MappedRefMut<'a, K, T> {
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
        (self.k, self.v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut T) {
        (self.k, self.v)
    }

    pub fn map<F, T2: ?Sized>(self, f: F) -> MappedRefMut<'a, K, T2>
    where
        F: FnOnce(&mut T) -> &mut T2,
    {
        MappedRefMut {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T2: ?Sized>(self, f: F) -> Result<MappedRefMut<'a, K, T2>, Self>
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

impl<'a, K: Eq + Hash + Debug, T: Debug + ?Sized> Debug for MappedRefMut<'a, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRefMut")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Eq + Hash, T: ?Sized> Deref for MappedRefMut<'a, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<'a, K: Eq + Hash, T: ?Sized> DerefMut for MappedRefMut<'a, K, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::DashMap;

    #[test]
    fn downgrade() {
        let data = DashMap::new();
        data.insert("test", "test");
        if let Some(mut w_ref) = data.get_mut("test") {
            *w_ref.value_mut() = "test2";
            let r_ref = w_ref.downgrade();
            assert_eq!(*r_ref.value(), "test2");
        };
    }

    #[test]
    fn mapped_mut() {
        let data = DashMap::new();
        data.insert("test", *b"test");
        if let Some(b_ref) = data.get_mut("test") {
            let mut s_ref = b_ref.try_map(|b| std::str::from_utf8_mut(b).ok()).unwrap();
            s_ref.value_mut().make_ascii_uppercase();
        }

        assert_eq!(data.get("test").unwrap().value(), b"TEST");
    }

    #[test]
    fn mapped_mut_again() {
        let data = DashMap::new();
        data.insert("test", *b"hello world");
        if let Some(b_ref) = data.get_mut("test") {
            let s_ref = b_ref.try_map(|b| std::str::from_utf8_mut(b).ok()).unwrap();
            let mut hello_ref = s_ref.try_map(|s| s.get_mut(..5)).unwrap();
            hello_ref.value_mut().make_ascii_uppercase();
        }

        assert_eq!(data.get("test").unwrap().value(), b"HELLO world");
    }

    #[test]
    fn mapped_ref() {
        let data = DashMap::new();
        data.insert("test", *b"test");
        if let Some(b_ref) = data.get("test") {
            let s_ref = b_ref.try_map(|b| std::str::from_utf8(b).ok()).unwrap();

            assert_eq!(s_ref.value(), "test");
        };
    }

    #[test]
    fn ref_map_split() {
        struct Data(String, String);
        let data = DashMap::new();
        data.insert("test", Data("hello".to_string(), "world".to_string()));
        if let Some(b_ref) = data.get("test") {
            let (l_ref, r_ref) = b_ref.map_split(|d| (&d.0, &d.1));

            assert_eq!(l_ref.value(), "hello");
            assert_eq!(r_ref.value(), "world");
        };
    }

    #[test]
    fn ref_mut_map_split() {
        let data = DashMap::new();
        data.insert("test", "hello world".to_string());
        if let Some(b_ref) = data.get_mut("test") {
            let (mut l_ref, r_ref) = b_ref.map_split(|d| d.split_at_mut(5));

            assert_eq!(l_ref.value(), "hello");
            assert_eq!(r_ref.value(), " world");
            l_ref.make_ascii_uppercase();
        };
        let Some(b_ref) = data.get("test") else { panic!("") };
        assert_eq!(b_ref.value(), "HELLO world");
    }

    #[test]
    fn mapped_ref_again() {
        let data = DashMap::new();
        data.insert("test", *b"hello world");
        if let Some(b_ref) = data.get("test") {
            let s_ref = b_ref.try_map(|b| std::str::from_utf8(b).ok()).unwrap();
            let hello_ref = s_ref.try_map(|s| s.get(..5)).unwrap();

            assert_eq!(hello_ref.value(), "hello");
        };
    }
}
