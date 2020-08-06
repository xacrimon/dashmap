use crate::alloc::{GlobalObjectAllocator, ObjectAllocator};
use std::ops::Deref;
use crate::gc::Gc;

pub struct Bucket<K, V, A: ObjectAllocator<Self>> {
    pub(crate) tag: A::Tag,
    pub(crate) key: K,
    pub(crate) value: V,
}

impl<K, V, A: ObjectAllocator<Self>> Bucket<K, V, A> {
    pub fn new(key: K, value: V) -> Self {
        Self {
            tag: A::Tag::default(),
            key,
            value,
        }
    }
}

/// A guard is a view of a map entry.
/// It exists to automatically manage memory behind the scenes.
pub struct Guard<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>> = GlobalObjectAllocator> {
    bucket: &'a Bucket<K, V, A>,
    gc: &'a Gc<Bucket<K, V, A>, A>,
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Guard<'a, K, V, A> {
    /// Returns the key associated with this entry.
    pub fn key(&self) -> &K {
        &self.bucket.key
    }

    /// Returns the value associated with this entry.
    pub fn value(&self) -> &V {
        &self.bucket.value
    }

    /// Returns both the key and the value associated with this entry.
    pub fn pair(&self) -> (&K, &V) {
        (&self.bucket.key, &self.bucket.value)
    }
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Deref for Guard<'a, K, V, A> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Clone for Guard<'a, K, V, A> {
    fn clone(&self) -> Self {
        self.gc.enter();

        Self {
            bucket: self.bucket,
            gc: self.gc
        }
    }
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Drop for Guard<'a, K, V, A> {
    fn drop(&mut self) {
        self.gc.exit();
    }
}
