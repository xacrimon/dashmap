//! Simple EBR garbage collector.
//! TO-DO: Research Stamp-it and Debra, optimize this garbage collector.
//! https://arxiv.org/abs/1805.08639
//! https://arxiv.org/pdf/1712.01044.pdf

use std::sync::{Mutex, Arc};
use std::thread::{self, ThreadId};

fn id() -> ThreadId {
    thread::current().id()
}

struct Global {
    state: Mutex<GlobalState>,
}

struct GlobalState {
    // Global epoch. This value is always 0, 1 or 2.
    epoch: usize,

    // Deferred functions.
    deferred: Vec<Box<dyn FnOnce()>>,

    // List of participants.
    locals: Vec<*const Local>,
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
