use crate::mapref;
use core::ops::Deref;
use std::collections::hash_map::RandomState;
pub struct RefMulti<'a, K, S = RandomState> {
    inner: mapref::multiple::RefMulti<'a, K, (), S>,
}

impl<'a, K, S> RefMulti<'a, K, S> {
    pub(crate) fn new(inner: mapref::multiple::RefMulti<'a, K, (), S>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> &K {
        self.inner.key()
    }
}

impl<'a, K, S> Deref for RefMulti<'a, K, S> {
    type Target = K;

    fn deref(&self) -> &K {
        self.key()
    }
}
