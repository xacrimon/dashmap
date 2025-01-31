use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::util::try_map;
use core::ops::{Deref, DerefMut};
use std::fmt::{Debug, Formatter};

pub struct Ref<'a, T> {
    _guard: RwLockReadGuardDetached<'a>,
    t: &'a T,
}

impl<'a, T> Ref<'a, T> {
    pub(crate) fn new(guard: RwLockReadGuardDetached<'a>, t: &'a T) -> Self {
        Self { _guard: guard, t }
    }

    pub fn value(&self) -> &T {
        self.t
    }

    pub fn map<F, U>(self, f: F) -> MappedRef<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        MappedRef {
            _guard: self._guard,
            t: f(self.t),
        }
    }

    pub fn try_map<F, U>(self, f: F) -> Result<MappedRef<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        if let Some(t) = f(self.t) {
            Ok(MappedRef {
                _guard: self._guard,
                t,
            })
        } else {
            Err(self)
        }
    }
}

impl<T: Debug> Debug for Ref<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.t.fmt(f)
    }
}

impl<T> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

pub struct RefMut<'a, T> {
    guard: RwLockWriteGuardDetached<'a>,
    t: &'a mut T,
}

impl<'a, T> RefMut<'a, T> {
    pub(crate) fn new(guard: RwLockWriteGuardDetached<'a>, t: &'a mut T) -> Self {
        Self { guard, t }
    }

    pub fn value(&self) -> &T {
        self.t
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.t
    }

    pub fn downgrade(self) -> Ref<'a, T> {
        Ref::new(
            // SAFETY: `Ref` will prevent writes to the data.
            unsafe { RwLockWriteGuardDetached::downgrade(self.guard) },
            self.t,
        )
    }

    pub fn map<F, U>(self, f: F) -> MappedRefMut<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        MappedRefMut {
            _guard: self.guard,
            t: f(self.t),
        }
    }

    pub fn try_map<F, U: 'a>(self, f: F) -> Result<MappedRefMut<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Self { guard, t } = self;
        match try_map(t, f) {
            Ok(t) => Ok(MappedRefMut { _guard: guard, t }),
            Err(t) => Err(Self { guard, t }),
        }
    }
}

impl<T: Debug> Debug for RefMut<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.t.fmt(f)
    }
}

impl<T> Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<T> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}

pub struct MappedRef<'a, T> {
    _guard: RwLockReadGuardDetached<'a>,
    t: &'a T,
}

impl<'a, T> MappedRef<'a, T> {
    pub fn value(&self) -> &T {
        self.t
    }

    pub fn map<F, T2>(self, f: F) -> MappedRef<'a, T2>
    where
        F: FnOnce(&T) -> &T2,
    {
        MappedRef {
            _guard: self._guard,
            t: f(self.t),
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRef<'a, T2>, Self>
    where
        F: FnOnce(&T) -> Option<&T2>,
    {
        let v = match f(self.t) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self._guard;
        Ok(MappedRef {
            _guard: guard,
            t: v,
        })
    }
}

impl<T: Debug> Debug for MappedRef<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.t.fmt(f)
    }
}

impl<T> Deref for MappedRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<T: std::fmt::Display> std::fmt::Display for MappedRef<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.value(), f)
    }
}

impl<T: AsRef<TDeref>, TDeref: ?Sized> AsRef<TDeref> for MappedRef<'_, T> {
    fn as_ref(&self) -> &TDeref {
        self.value().as_ref()
    }
}

pub struct MappedRefMut<'a, T> {
    _guard: RwLockWriteGuardDetached<'a>,
    t: &'a mut T,
}

impl<'a, T> MappedRefMut<'a, T> {
    pub fn value(&self) -> &T {
        self.t
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.t
    }

    pub fn map<F, T2>(self, f: F) -> MappedRefMut<'a, T2>
    where
        F: FnOnce(&mut T) -> &mut T2,
    {
        MappedRefMut {
            _guard: self._guard,
            t: f(self.t),
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRefMut<'a, T2>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut T2>,
    {
        let Self { _guard, t } = self;
        match try_map(t, f) {
            Ok(t) => Ok(MappedRefMut { _guard, t }),
            Err(t) => Err(Self { _guard, t }),
        }
    }
}

impl<T: Debug> Debug for MappedRefMut<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.t.fmt(f)
    }
}

impl<T> Deref for MappedRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<T> DerefMut for MappedRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}
