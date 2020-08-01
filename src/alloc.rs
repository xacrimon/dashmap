//! The `alloc` module contains types and traits for
//! customizing the memory allocation behaviour of the map.

use std::alloc::{alloc, dealloc, Layout};
use std::ptr;

/// This trait defines the interface for the object allocator the map uses to allocate buckets.
/// Bucket allocators should be swappable to the needs of the user.
/// The `V` type parameter designates the object type.
pub trait ObjectAllocator<V> {
    /// The `Tag` associated type is a unique tag that can be used to identify an allocation.
    type Tag: Default;

    /// Allocates a new object and returns a identifying tag and a pointer.
    /// The pointer must the valid for the lifetime of the allocator.
    fn allocate(&self, item: V) -> (Self::Tag, *mut V);

    /// Deallocates an object via its tag.
    fn deallocate(&self, tag: &Self::Tag);
}

/// The default object allocator. This is merely a typed wrapper around the global allocator.
/// This shouldn't have any unexpected properties and is a balanced choice,
/// only crippled by the occasionally mediocre performance of the global allocator.
pub struct GlobalObjectAllocator;

impl<V> ObjectAllocator<V> for GlobalObjectAllocator {
    type Tag = usize;

    fn allocate(&self, item: V) -> (Self::Tag, *mut V) {
        let layout = Layout::new::<V>();
        let ptr = unsafe { alloc(layout) } as *mut V;
        unsafe { ptr::write(ptr, item) }
        (ptr as usize, ptr)
    }

    fn deallocate(&self, tag: &Self::Tag) {
        let ptr = *tag as *mut V;
        unsafe { ptr::drop_in_place(ptr) }
        let layout = Layout::new::<V>();
        unsafe {
            dealloc(ptr as _, layout);
        }
    }
}
