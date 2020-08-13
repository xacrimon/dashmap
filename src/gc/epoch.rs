use crate::utils::{
    hint::unreachable,
    shim::sync::atomic::{AtomicUsize, Ordering},
};

/// Represents a valid state epoch.
/// Since we only have 4 of them we can safely represent them with an enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Epoch {
    Zero,
    One,
    Two,
    Three,
}

impl Epoch {
    /// Get the next valid epoch. This wraps as it transitions from `3 -> 0`.
    pub fn next(self) -> Self {
        match self {
            Self::Zero => Self::One,
            Self::One => Self::Two,
            Self::Two => Self::Three,
            Self::Three => Self::Zero,
        }
    }

    /// Create an epoch from a raw integer value.
    ///
    /// # Safety
    /// This function may only be called with integers 0..4.
    unsafe fn from_usize_unchecked(raw: usize) -> Self {
        match raw {
            0 => Self::Zero,
            1 => Self::One,
            2 => Self::Two,
            3 => Self::Three,
            _ => unreachable(),
        }
    }
}

/// Convert an epoch into it's raw representation.
impl Into<usize> for Epoch {
    fn into(self) -> usize {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

/// An atomic epoch value.
pub struct AtomicEpoch {
    raw: AtomicUsize,
}

impl AtomicEpoch {
    /// Create a new atomic epoch with a starting value.
    pub fn new(epoch: Epoch) -> Self {
        Self {
            raw: AtomicUsize::new(epoch.into()),
        }
    }

    /// Load the epoch from the atomic with relaxed ordering.
    pub fn load(&self) -> Epoch {
        let raw = self.raw.load(Ordering::Relaxed);
        unsafe { Epoch::from_usize_unchecked(raw) }
    }

    /// Load the epoch from the atomic with acquire ordering.
    pub fn load_acquire(&self) -> Epoch {
        let raw = self.raw.load(Ordering::Acquire);
        unsafe { Epoch::from_usize_unchecked(raw) }
    }

    /// Store an epoch into the atomic with relaxed ordering.
    pub fn store(&self, epoch: Epoch) {
        let raw: usize = epoch.into();
        self.raw.store(raw, Ordering::Relaxed);
    }

    /// Try to advance the epoch in this atomic.
    /// On success it returns the new epoch.
    /// The atomic value is not updated on error.
    pub fn try_advance(&self, current: Epoch) -> Result<Epoch, ()> {
        let current_raw: usize = current.into();
        let next = current.next();
        let next_raw: usize = next.into();

        // we need to use acq_rel here in order for this to synchronize properly with
        // attempts to advance in other threads and certain code that
        // must have the correct epoch performing an acquire load
        //
        // in practice since the relative frequency of calls to this function
        // is low this should not have problematic impact on performance
        let did_advance = self.raw.compare_exchange_weak(
            current_raw,
            next_raw,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );

        if did_advance.is_ok() {
            Ok(next)
        } else {
            Err(())
        }
    }
}
