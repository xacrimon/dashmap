//! The `alloc` module contains types and traits for
//! customizing the memory allocation behaviour of the map.

use std::alloc::{alloc, dealloc, Layout};
use std::ptr;

/// This trait defines the interface for the object allocator the map uses to allocate buckets.
/// Bucket allocators should be swappable to the needs of the user.
/// The `V` type parameter designates the object type.
pub trait ObjectAllocator<V>: Send + Sync {
    /// The `Tag` associated type is a unique tag that can be used to identify an allocation.
    type Tag: Copy + Default;

    /// Allocates a new object and returns a identifying tag and a pointer.
    /// The pointer must the valid for the lifetime of the allocator.
    fn allocate(&self, item: V) -> (Self::Tag, *mut V);

    /// Deallocates an object with a specific tag.
    ///
    /// # Safety
    /// An allocation may only be deallocated exactly once, hence this function is unsafe.
    /// Calling this function multiple times for the same allocation is undefined behaviour.
    ///
    /// Note that you may call this function multiple times with the same tag
    /// if you've received the same tag multiple times from `allocate`.
    /// This may happen as tag reuse is allowed.
    unsafe fn deallocate(&self, tag: Self::Tag);
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

        // # Safety
        // The block of memory we are writing to is freshly allocated with the correct layout.
        // Since there is nothing there we can safely perform a raw write.
        unsafe { ptr::write(ptr, item) }

        (ptr as usize, ptr)
    }

    unsafe fn deallocate(&self, tag: Self::Tag) {
        let ptr = tag as *mut V;
        let layout = Layout::new::<V>();

        // # Safety
        // We call `drop_in_place` here to drop the value at this memory location.
        // This is safe to do since it is guaranteed to be initialized.
        ptr::drop_in_place(ptr);

        // # Safety
        // Here we dellocate the memory which is fine to do as it is guaranteed
        // to be allocated assuming the implementation of `allocate` is valid.
        // `dealloc` does not perform any drop calls, that's why we did that manually earlier.
        dealloc(ptr as _, layout);
    }
}

#[cfg(test)]
mod tests {
    use super::{GlobalObjectAllocator, ObjectAllocator};

    #[test]
    fn alloc_dealloc() {
        let allocator = GlobalObjectAllocator;
        let (tag, ptr) =
            <GlobalObjectAllocator as ObjectAllocator<usize>>::allocate(&allocator, 55);

        // the allocator should never return null
        assert!(!ptr.is_null());

        unsafe {
            <GlobalObjectAllocator as ObjectAllocator<usize>>::deallocate(&allocator, tag);
        }
    }
}
