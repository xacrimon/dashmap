use crate::alloc::{GlobalObjectAllocator, ObjectAllocator};
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct Bucket<K, V, A: ObjectAllocator<Self>> {
    refs: AtomicU32,
    tag: A::Tag,
    key: K,
    value: V,
}

impl<K, V, A: ObjectAllocator<Self>> Bucket<K, V, A> {
    pub fn new(key: K, value: V) -> Self {
        Self {
            refs: AtomicU32::new(1),
            tag: A::Tag::default(),
            key,
            value,
        }
    }

    pub fn add_ref(&self) {
        self.refs.fetch_add(1, Ordering::SeqCst);
    }

    pub fn fetch_sub_ref(&self) -> u32 {
        self.refs.fetch_sub(1, Ordering::SeqCst)
    }

    pub fn key(&self) -> &K {
        &self.key
    }
}

/// A guard is a view of a map entry.
/// It exists to automatically manage memory behind the scenes.
pub struct Guard<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>> = GlobalObjectAllocator> {
    bucket: &'a Bucket<K, V, A>,
    allocator: &'a A,
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
        self.bucket.add_ref();
        Self {
            bucket: self.bucket,
            allocator: self.allocator,
        }
    }
}

impl<'a, K, V, A: ObjectAllocator<Bucket<K, V, A>>> Drop for Guard<'a, K, V, A> {
    fn drop(&mut self) {
        let previous_refs = self.bucket.fetch_sub_ref();

        if previous_refs == 1 {
            todo!("defer this");
            self.allocator.deallocate(&self.bucket.tag);
        }
    }
}
