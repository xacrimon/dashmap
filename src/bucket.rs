use crate::alloc::{GlobalObjectAllocator, ObjectAllocator};
use std::ops::Deref;
use flize::{ebr::Ebr, function_runner::FunctionRunner, Shield};

/// Represents an occupied slot in a map.
/// Besides the key and value it contains the allocator tag
/// so it can deallocate itself when it's lifetime ends.
pub struct Bucket<K, V, A: ObjectAllocator<Self>> {
    pub(crate) tag: A::Tag,
    pub(crate) key: K,
    pub(crate) value: V,
}

impl<K, V, A: ObjectAllocator<Self>> Bucket<K, V, A> {
    /// Create a new bucket with a default tag.
    /// The tag will need to be set to the correct value before being published to the map.
    pub fn new(key: K, value: V) -> Self {
        Self {
            tag: A::Tag::default(),
            key,
            value,
        }
    }

    pub fn read<'a>(&'a self, shield: Shield<'a, Ebr<FunctionRunner>>) -> Guard<'a, K, V, A> {
        Guard::new(self, shield)
    }
}

/// A guard is a view of a map entry.
/// It exists to automatically manage memory behind the scenes.
pub struct Guard<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>> = GlobalObjectAllocator> {
    bucket: &'a Bucket<K, V, A>,
    shield: Shield<'a, Ebr<FunctionRunner>>,
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Guard<'a, K, V, A> {
    fn new(bucket: &'a Bucket<K, V, A>, shield: Shield<'a, Ebr<FunctionRunner>>) -> Self {
        Self { bucket, shield }
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
        Self {
            bucket: self.bucket,
            shield: self.shield.clone(),
        }
    }
}
