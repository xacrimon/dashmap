//! Simple EBR garbage collector.
//! TO-DO: Research Stamp-it and DEBRA, optimize this garbage collector.
//! https://arxiv.org/abs/1805.08639
//! https://arxiv.org/pdf/1712.01044.pdf

use std::mem::take;
use std::sync::{Arc, Mutex};

struct Deferred {
    task: Box<dyn FnOnce()>,
}

impl Deferred {
    fn new(f: impl FnOnce() + 'static) -> Self {
        Self { task: Box::new(f) }
    }

    fn run(self) {
        (self.task)();
    }
}

unsafe impl Send for Deferred {}
unsafe impl Sync for Deferred {}

fn calc_free_epoch(a: usize) -> usize {
    (a + 3 - 2) % 3
}

struct Global {
    state: Mutex<GlobalState>,
}

struct GlobalState {
    // Global epoch. This value is always 0, 1 or 2.
    epoch: usize,

    // Deferred functions.
    deferred: [Vec<Deferred>; 3],

    // List of participants.
    locals: Vec<*const Local>,
}

impl Global {
    fn collect(&self) {
        let mut guard = self.state.lock().unwrap();
        let mut state = &mut *guard;
        let mut can_collect = true;

        for local_ptr in &state.locals {
            unsafe {
                let local = &**local_ptr;
                let local_state = local.state.lock().unwrap();
                if local_state.active {
                    if local_state.epoch != state.epoch {
                        can_collect = false;
                    }
                }
            }
        }

        if can_collect {
            state.epoch = (state.epoch + 1) % 3;
            let free_epoch = calc_free_epoch(state.epoch);
            let free_deferred = take(&mut state.deferred[free_epoch]);

            for deferred in free_deferred {
                deferred.run();
            }
        }
    }
}

struct Local {
    state: Mutex<LocalState>,
}

struct LocalState {
    // Active flag.
    active: bool,

    // Local epoch. This value is always 0, 1 or 2.
    epoch: usize,

    // Reference to global state.
    global: Arc<Global>,
}

impl Local {
    fn enter_critical(&self) {
        let mut state = self.state.lock().unwrap();
        assert!(!state.active);
        state.active = true;
        let global = state.global.state.lock().unwrap().epoch;
        state.epoch = global;
    }

    fn exit_critical(&self) {
        let mut state = self.state.lock().unwrap();
        assert!(state.active);
        state.active = false;
    }

    fn defer(&self, f: Deferred) {
        let local_state = self.state.lock().unwrap();
        assert!(local_state.active);
        let mut global_state = local_state.global.state.lock().unwrap();
        let global_epoch = global_state.epoch;
        global_state.deferred[global_epoch].push(f);
    }
}
