use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use core::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct RefMulti<'a, T> {
    _guard: Arc<RwLockReadGuardDetached<'a>>,
    t: &'a T,
}

impl<T> Clone for RefMulti<'_, T> {
    fn clone(&self) -> Self {
        Self {
            _guard: self._guard.clone(),
            t: self.t,
        }
    }
}

impl<'a, T> RefMulti<'a, T> {
    pub(crate) fn new(guard: Arc<RwLockReadGuardDetached<'a>>, v: &'a T) -> Self {
        Self {
            _guard: guard,
            t: v,
        }
    }

    pub fn value(&self) -> &T {
        self.t
    }
}

impl<T> Deref for RefMulti<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

pub struct RefMutMulti<'a, T> {
    _guard: Arc<RwLockWriteGuardDetached<'a>>,
    t: &'a mut T,
}

impl<'a, T> RefMutMulti<'a, T> {
    pub(crate) fn new(guard: Arc<RwLockWriteGuardDetached<'a>>, t: &'a mut T) -> Self {
        Self { _guard: guard, t }
    }

    pub fn value(&self) -> &T {
        self.t
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.t
    }
}

impl<T> Deref for RefMutMulti<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<T> DerefMut for RefMutMulti<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}
