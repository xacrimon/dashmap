//! Simple EBR garbage collector.
//! TO-DO: Optimize this garbage collector.
//!        Research Stamp-it, DEBRA, Hazard Eras.

use once_cell::sync::Lazy;
use once_cell::unsync::Lazy as UnsyncLazy;
use std::mem::take;
use std::ops::Deref;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Execute a closure in protected mode. This permits it to load protected pointers.
pub fn protected<T>(f: impl FnOnce() -> T) -> T {
    PARTICIPANT_HANDLE.with(|key| key.enter_critical());
    let r = f();
    PARTICIPANT_HANDLE.with(|key| key.exit_critical());
    r
}

/// Defer a function.
pub fn defer(f: impl FnOnce()) {
    let deferred = Deferred::new(f);
    PARTICIPANT_HANDLE.with(|key| key.defer(deferred));
}

/// Collect garbage.
pub fn collect() {
    GC.collect();
}

static GC: Lazy<Arc<Global>> = Lazy::new(|| Arc::new(Global::new()));

thread_local! {
    pub static PARTICIPANT_HANDLE: UnsyncLazy<TSLocal> = UnsyncLazy::new(|| TSLocal::new(Arc::clone(&GC)));
}

pub struct TSLocal {
    local: Box<Local>,
}

impl TSLocal {
    fn new(global: Arc<Global>) -> TSLocal {
        let local = Box::new(Local::new(Arc::clone(&global)));
        let local_ptr = &*local as *const Local;
        global.add_local(local_ptr);
        Self { local }
    }
}

impl Deref for TSLocal {
    type Target = Local;

    fn deref(&self) -> &Self::Target {
        &self.local
    }
}

impl Drop for TSLocal {
    fn drop(&mut self) {
        let global = Arc::clone(&self.local.global);
        let local_ptr = &*self.local as *const Local;
        global.remove_local(local_ptr);
    }
}

struct Deferred {
    task: [usize; 2],
}

impl Deferred {
    fn new<'a>(f: impl FnOnce() + 'a) -> Self {
        let boxed: Box<dyn FnOnce() + 'a> = Box::new(f);
        let fat_ptr = Box::into_raw(boxed);
        let mut task = [0; 2];
        unsafe {
            ptr::write(&mut task as *mut [usize; 2] as usize as _, fat_ptr);
        };

        Self { task }
    }

    fn run(mut self) {
        unsafe {
            let fat_ptr: *mut dyn FnOnce() =
                ptr::read(&mut self.task as *mut [usize; 2] as usize as _);
            let boxed = Box::from_raw(fat_ptr);
            boxed();
        }
    }
}

unsafe impl Send for Deferred {}
unsafe impl Sync for Deferred {}

fn calc_free_epoch(a: usize) -> usize {
    (a + 3 - 2) % 3
}

struct Global {
    // Global epoch. This value is always 0, 1 or 2.
    epoch: Mutex<usize>,

    // Other state.
    state: Mutex<GlobalState>,
}

unsafe impl Send for Global {}
unsafe impl Sync for Global {}

struct GlobalState {
    // Deferred functions.
    deferred: [Vec<Deferred>; 3],

    // List of participants.
    locals: Vec<*const Local>,
}

impl Global {
    fn new() -> Self {
        Self {
            epoch: Mutex::new(0),
            state: Mutex::new(GlobalState {
                deferred: [Vec::new(), Vec::new(), Vec::new()],
                locals: Vec::new(),
            }),
        }
    }

    fn add_local(&self, local: *const Local) {
        self.state.lock().unwrap().locals.push(local);
    }

    fn remove_local(&self, local: *const Local) {
        self.state
            .lock()
            .unwrap()
            .locals
            .retain(|maybe_this| *maybe_this != local);
    }

    fn collect(&self) {
        let mut global_epoch = self.epoch.lock().unwrap();
        let mut guard = self.state.lock().unwrap();
        let state = &mut *guard;
        let mut can_collect = true;

        for local_ptr in &state.locals {
            unsafe {
                let local = &**local_ptr;
                if local.active.load(Ordering::SeqCst) > 0 {
                    if local.epoch.load(Ordering::SeqCst) != *global_epoch {
                        can_collect = false;
                    }
                }
            }
        }

        if can_collect {
            *global_epoch = (*global_epoch + 1) % 3;
            let free_epoch = calc_free_epoch(*global_epoch);
            let free_deferred = take(&mut state.deferred[free_epoch]);
            drop(global_epoch);

            for deferred in free_deferred {
                deferred.run();
            }
        }
    }
}

pub struct Local {
    // Active flag.
    active: AtomicUsize,

    // Local epoch. This value is always 0, 1 or 2.
    epoch: AtomicUsize,

    // Reference to global state.
    global: Arc<Global>,
}

impl Local {
    fn new(global: Arc<Global>) -> Self {
        Self {
            active: AtomicUsize::new(0),
            epoch: AtomicUsize::new(0),
            global,
        }
    }

    pub fn enter_critical(&self) {
        if self.active.fetch_add(1, Ordering::SeqCst) == 0 {
            let global_epoch = self.global.epoch.lock().unwrap();
            self.epoch.store(*global_epoch, Ordering::SeqCst);
        }
    }

    pub fn exit_critical(&self) {
        if self.active.fetch_sub(1, Ordering::SeqCst) == 0 {
            panic!("uh oh");
        }
    }

    fn defer(&self, f: Deferred) {
        let active = self.active.load(Ordering::SeqCst);
        debug_assert!(active > 0);
        let global_epoch = self.global.epoch.lock().unwrap();
        let mut global_state = self.global.state.lock().unwrap();
        global_state.deferred[*global_epoch].push(f);
    }
}
