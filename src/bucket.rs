use crate::alloc::{GlobalObjectAllocator, ObjectAllocator};
use crate::gc::Gc;
use std::ops::Deref;

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

    /// This assumes you've already entered a critical section.
    /// Since guards exit a critical section on drop it is UB to not be
    /// in a critical section when this is calling.
    pub fn read<'a>(&'a self, gc: &'a Gc<Bucket<K, V, A>, A>) -> Guard<'a, K, V, A> {
        Guard::new(self, gc)
    }
}

/// A guard is a view of a map entry.
/// It exists to automatically manage memory behind the scenes.
pub struct Guard<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>> = GlobalObjectAllocator> {
    bucket: &'a Bucket<K, V, A>,
    gc: &'a Gc<Bucket<K, V, A>, A>,
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Guard<'a, K, V, A> {
    fn new(bucket: &'a Bucket<K, V, A>, gc: &'a Gc<Bucket<K, V, A>, A>) -> Self {
        Self { bucket, gc }
    }

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
            gc: self.gc,
        }
    }
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Drop for Guard<'a, K, V, A> {
    fn drop(&mut self) {
        self.gc.exit();
    }
}
