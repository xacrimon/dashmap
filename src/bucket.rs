use crate::alloc::{GlobalObjectAllocator, ObjectAllocator};
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct Bucket<K, V, A: ObjectAllocator<Self>> {
    refs: AtomicU32,
    tag: A::Tag,
    pub(crate) key: K,
    pub(crate) value: V,
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

    /// Increments the reference count by one.
    pub fn add_ref(&self) {
        self.refs.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements the reference count by one and returns the new reference count.
    /// Deallocates the bucket if it is 0.
    pub unsafe fn sub_ref(&self, allocator: &A) -> u32 {
        let ref_count = self.refs.fetch_sub(1, Ordering::SeqCst) - 1;

        if ref_count == 0 {
            todo!("defer this");
            allocator.deallocate(&self.tag);
        }

        return ref_count;
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
        // # Safety
        // This is safe to do since we are destroying the reference object.
        // The reference object increments the reference count when created so we simply remove our slot.
        unsafe {
            self.bucket.sub_ref(self.allocator);
        }
    }
}
