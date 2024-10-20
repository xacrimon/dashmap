use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};
use seize::CachePadded;
use std::convert::TryInto;

// Polyfill for the unstable strict-provenance APIs.
#[allow(clippy::missing_safety_doc)]
pub unsafe trait StrictProvenance<T>: Sized {
    fn addr(self) -> usize;
    fn map_addr(self, f: impl FnOnce(usize) -> usize) -> Self;
    fn unpack(self) -> Tagged<T>
    where
        T: Unpack;
}

// Unpack a tagged pointer.
pub trait Unpack {
    // A mask for the pointer tag bits.
    const MASK: usize;
}

unsafe impl<T> StrictProvenance<T> for *mut T {
    #[inline(always)]
    fn addr(self) -> usize {
        self as usize
    }

    #[inline(always)]
    fn map_addr(self, f: impl FnOnce(usize) -> usize) -> Self {
        f(self.addr()) as Self
    }

    #[inline(always)]
    fn unpack(self) -> Tagged<T>
    where
        T: Unpack,
    {
        Tagged {
            raw: self,
            ptr: self.map_addr(|addr| addr & T::MASK),
        }
    }
}

// An unpacked tagged pointer.
pub struct Tagged<T> {
    // The raw, tagged pointer.
    pub raw: *mut T,
    // The untagged pointer.
    pub ptr: *mut T,
}

// Creates a `Tagged` from an untagged pointer.
#[inline]
pub fn untagged<T>(value: *mut T) -> Tagged<T> {
    Tagged {
        raw: value,
        ptr: value,
    }
}

impl<T> Tagged<T>
where
    T: Unpack,
{
    // Returns the tag portion of this pointer.
    #[inline]
    pub fn tag(self) -> usize {
        self.raw.addr() & !T::MASK
    }

    // Maps the tag of this pointer.
    #[inline]
    pub fn map_tag(self, f: impl FnOnce(usize) -> usize) -> Self {
        Tagged {
            raw: self.raw.map_addr(f),
            ptr: self.ptr,
        }
    }
}

impl<T> Copy for Tagged<T> {}

impl<T> Clone for Tagged<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// Polyfill for the unstable `atomic_ptr_strict_provenance` APIs.
pub trait AtomicPtrFetchOps<T> {
    fn fetch_or(&self, value: usize, ordering: Ordering) -> *mut T;
}

impl<T> AtomicPtrFetchOps<T> for AtomicPtr<T> {
    #[inline]
    fn fetch_or(&self, value: usize, ordering: Ordering) -> *mut T {
        #[cfg(not(miri))]
        {
            use std::sync::atomic::AtomicUsize;

            unsafe { &*(self as *const AtomicPtr<T> as *const AtomicUsize) }
                .fetch_or(value, ordering) as *mut T
        }

        // Avoid ptr2int under Miri.
        #[cfg(miri)]
        {
            // Returns the ordering for the read in an RMW operation.
            const fn read_ordering(ordering: Ordering) -> Ordering {
                match ordering {
                    Ordering::SeqCst => Ordering::SeqCst,
                    Ordering::AcqRel => Ordering::Acquire,
                    _ => Ordering::Relaxed,
                }
            }

            self.fetch_update(ordering, read_ordering(ordering), |ptr| {
                Some(ptr.map_addr(|addr| addr | value))
            })
            .unwrap()
        }
    }
}

// A sharded atomic counter.
pub struct Counter(Box<[CachePadded<AtomicIsize>]>);

impl Default for Counter {
    fn default() -> Counter {
        let num_cpus = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let shards = (0..num_cpus.next_power_of_two())
            .map(|_| Default::default())
            .collect();
        Counter(shards)
    }
}

impl Counter {
    // Return the shard for the given thread ID.
    #[inline]
    pub fn get(&self, thread: usize) -> &AtomicIsize {
        &self.0[thread & (self.0.len() - 1)].value
    }

    // Returns the sum of all counter shards.
    #[inline]
    pub fn sum(&self) -> usize {
        self.0
            .iter()
            .map(|x| x.value.load(Ordering::Relaxed))
            .sum::<isize>()
            .try_into()
            // Depending on the order of deletion/insertions this might be negative,
            // so assume the map is empty.
            .unwrap_or(0)
    }
}

// `Box<T>` but aliasable.
pub struct Shared<T>(NonNull<T>);

impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Shared<T> {
        Shared(unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(value))) })
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0.as_ptr() }
    }
}

impl<T> Drop for Shared<T> {
    #[inline]
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.0.as_ptr()) };
    }
}
