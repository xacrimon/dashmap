use super::epoch::{AtomicEpoch, Epoch};
use crate::alloc::ObjectAllocator;
use crate::utils::{
    shim::sync::atomic::{AtomicUsize, Ordering},
    wyrng::WyRng,
};
use std::cell::UnsafeCell;
use std::marker::PhantomData;

const COLLECT_CHANCE: u32 = 4;

/// The interface we need in order to work with the main GC state.
pub trait EbrState {
    type T;
    type A: ObjectAllocator<Self::T>;

    fn load_epoch(&self) -> Epoch;
    fn should_advance(&self) -> bool;
    fn try_cycle(&self);
}

/// Per thread state needed for the GC.
/// We store a local epoch, an active flag and a number generator used
/// for reducing the frequency of some operations.
pub struct ThreadState<G> {
    active: AtomicUsize,
    epoch: AtomicEpoch,
    rng: UnsafeCell<WyRng>,
    _m0: PhantomData<G>,
}

impl<G: EbrState> ThreadState<G> {
    pub fn new(state: &G, thread_id: u32) -> Self {
        let global_epoch = state.load_epoch();

        Self {
            active: AtomicUsize::new(0),
            epoch: AtomicEpoch::new(global_epoch),
            rng: UnsafeCell::new(WyRng::new(thread_id)),
            _m0: PhantomData,
        }
    }

    /// Check if we should try to advance the global epoch.
    ///
    /// We use random numbers here to reduce the frequency of this returning true.
    /// We do this because advancing the epoch is a rather expensive operation.
    ///
    /// # Safety
    /// This function may only be called from the thread this state belongs to.
    /// This is due to the fact that it will access the thread-local
    /// PRNG without synchronization. 
    unsafe fn should_advance(&self, state: &G) -> bool {
        let rng = &mut *self.rng.get();
        (rng.generate() % COLLECT_CHANCE == 0) && state.should_advance()
    }

    /// Check if the given thread is in a critical section.
    pub fn is_active(&self) -> bool {
        // acquire is used here so it is not reordered after a call to `enter`
        // it's fine if we get some false positives here
        // but false negatives would cause undefined behaviour
        // because the global epoch is illegally advanced
        self.active.load(Ordering::Acquire) == 0
    }

    /// Get the local epoch of the given thread with relaxed ordering.
    /// It's fine to use relaxed here since the worst case scenario is that
    /// a thread attempting to advance the global epoch sees an old local epoch
    /// which will cause the attempt to fail anyway.
    pub fn load_epoch(&self) -> Epoch {
        self.epoch.load()
    }

    /// Enter a critical section with the given thread.
    ///
    /// # Safety
    /// This function may only be called from the thread this state belongs to.
    pub unsafe fn enter(&self, state: &G) {
        // since `active` is a counter we only need to
        // update the local epoch when we go from 0 to something else
        //
        // release is used here because this may not be reordered with a call to `is_active`
        // that could cause a thread to advance the global epoch illegally
        if self.active.fetch_add(1, Ordering::Release) == 0 {
            // relaxed is fine here since loading an old global
            // cannot cause an illegal epoch advance
            let global_epoch = state.load_epoch();

            // relaxed is here because there is only one thread
            // that may write to this variable
            self.epoch.store(global_epoch);
        }
    }

    /// Exit a critical section with the given thread.
    ///
    /// # Safety
    /// This function may only be called from the thread this state belongs to.
    pub unsafe fn exit(&self, state: &G) {
        // decrement the `active` counter and fetch the previous value
        //
        // relaxed is fine to use here since there is only one local thread that may
        // call `enter` and `exit` and thus any funny reorderings between
        // `enter` and `exit are impossible
        let prev_active = self.active.fetch_sub(1, Ordering::Relaxed);

        // if the counter wraps we've called exit more than enter which is not allowed
        debug_assert!(prev_active != 0);

        // check if we should try to advance the epoch if it reaches 0
        if prev_active == 1 {
            if self.should_advance(state) {
                state.try_cycle();
            }
        }
    }
}

unsafe impl<G: Sync> Sync for ThreadState<G> {}
