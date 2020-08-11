use super::epoch::{AtomicEpoch, Epoch};
use crate::alloc::ObjectAllocator;
use crate::utils::{
    pcg32::Pcg32,
    shim::sync::atomic::{AtomicUsize, Ordering, fence},
};
use std::cell::UnsafeCell;
use std::marker::PhantomData;

const COLLECT_CHANCE: u32 = 4;

pub trait EbrState {
    type T;
    type A: ObjectAllocator<Self::T>;

    fn load_epoch(&self) -> Epoch;
    fn should_advance(&self) -> bool;
    fn try_cycle(&self);
}

pub struct ThreadState<G> {
    active: AtomicUsize,
    epoch: AtomicEpoch,
    rng: UnsafeCell<Pcg32>,
    _m0: PhantomData<G>,
}

impl<G: EbrState> ThreadState<G> {
    pub fn new(state: &G, thread_id: u32) -> Self {
        let global_epoch = state.load_epoch();

        Self {
            active: AtomicUsize::new(0),
            epoch: AtomicEpoch::new(global_epoch),
            rng: UnsafeCell::new(Pcg32::new(thread_id)),
            _m0: PhantomData,
        }
    }

    fn should_advance(&self, state: &G) -> bool {
        let rng = unsafe { &mut *self.rng.get() };
        (rng.generate() % COLLECT_CHANCE == 0) && state.should_advance()
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst) == 0
    }

    pub fn load_epoch(&self) -> Epoch {
        self.epoch.load()
    }

    pub fn enter(&self, state: &G) {
        if self.active.fetch_add(1, Ordering::SeqCst) == 0 {
            let global_epoch = state.load_epoch();
            self.epoch.store(global_epoch);
            fence(Ordering::SeqCst);
        }
    }

    pub fn exit(&self, state: &G) {
        if self.active.fetch_sub(1, Ordering::SeqCst) == 1 {
            if self.should_advance(state) {
                state.try_cycle();
            }
        }
    }
}

unsafe impl<G: Sync> Sync for ThreadState<G> {}
