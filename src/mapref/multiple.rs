use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::util::SharedValue;
use core::hash::Hash;
use core::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct RefMulti<'a, K, V> {
    _guard: Arc<RwLockReadGuardDetached<'a>>,
    data: &'a (K, SharedValue<V>),
}

impl<'a, K: Eq + Hash, V> RefMulti<'a, K, V> {
    pub(crate) unsafe fn new(
        guard: Arc<RwLockReadGuardDetached<'a>>,
        data: &'a (K, SharedValue<V>),
    ) -> Self {
        Self {
            _guard: guard,
            data,
        }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &V) {
        (&self.data.0, self.data.1.get())
    }
}

impl<'a, K: Eq + Hash, V> Deref for RefMulti<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMutMulti<'a, K, V> {
    _guard: Arc<RwLockWriteGuardDetached<'a>>,
    data: &'a mut (K, SharedValue<V>),
}

impl<'a, K: Eq + Hash, V> RefMutMulti<'a, K, V> {
    pub(crate) unsafe fn new(
        guard: Arc<RwLockWriteGuardDetached<'a>>,
        data: &'a mut (K, SharedValue<V>),
    ) -> Self {
        Self {
            _guard: guard,
            data,
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
        (&self.data.0, self.data.1.get())
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        (&self.data.0, self.data.1.get_mut())
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
