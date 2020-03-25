use crate::mapref;
use ahash::RandomState;
use std::hash::{BuildHasher, Hash};
use std::ops::Deref;

pub struct Ref<'a, K, S = RandomState> {
    inner: mapref::one::Ref<'a, K, (), S>
}

unsafe impl<'a, K: Eq + Hash + Send, S: BuildHasher> Send for Ref<'a, K, S> {}
unsafe impl<'a, K: Eq + Hash + Send + Sync, S: BuildHasher> Sync
    for Ref<'a, K, S>
{
}

impl<'a, K: Eq + Hash, S: BuildHasher> Ref<'a, K, S> {
    #[inline(always)]
    pub(crate) fn new(inner: mapref::one::Ref<'a, K, (), S>) -> Self {
        Self { inner }
    }
    #[inline(always)]
    pub fn key(&self) -> &K {
        self.inner.key()
    }
}

impl<'a, K: Eq + Hash, S: BuildHasher> Deref for Ref<'a, K, S> {
    type Target = K;
    #[inline(always)]
    fn deref(&self) -> &K {
        self.key()
    }
}
