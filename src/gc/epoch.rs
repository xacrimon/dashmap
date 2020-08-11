use crate::utils::{
    shim::sync::atomic::{AtomicUsize, Ordering},
    unreachable::unreachable,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Epoch {
    Zero,
    One,
    Two,
    Three,
}

impl Epoch {
    pub fn next(self) -> Self {
        match self {
            Self::Zero => Self::One,
            Self::One => Self::Two,
            Self::Two => Self::Three,
            Self::Three => Self::Zero,
        }
    }

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

pub struct AtomicEpoch {
    raw: AtomicUsize,
}

impl AtomicEpoch {
    pub fn new(epoch: Epoch) -> Self {
        Self {
            raw: AtomicUsize::new(epoch.into()),
        }
    }

    pub fn load(&self) -> Epoch {
        let raw = self.raw.load(Ordering::SeqCst);
        unsafe { Epoch::from_usize_unchecked(raw) }
    }

    pub fn store(&self, epoch: Epoch) {
        let raw: usize = epoch.into();
        self.raw.store(raw, Ordering::SeqCst);
    }

    pub fn try_advance(&self) -> Result<Epoch, ()> {
        let current = self.load();
        let current_raw: usize = current.into();
        let next = current.next();
        let next_raw: usize = next.into();

        let did_advance = self.raw.compare_exchange_weak(
            current_raw,
            next_raw,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );

        if did_advance.is_ok() {
            Ok(next)
        } else {
            Err(())
        }
    }
}
