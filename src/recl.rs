//! Simple QSBR garbage collector.
//! This probably isn't the optimal model for memory reclamation. Stamp-it, Hazard Eras and EBR should be considered.

use std::sync::{Mutex, MutexGuard, Arc};
use std::mem::{swap, take};
use std::thread::{ThreadId, current};
use std::collections::HashMap;

type GarbageList = Vec<Box<dyn FnOnce()>>;

fn id() -> ThreadId {
    current().id()
}

fn collect<'a>(mut state: MutexGuard<'a, GcState>) -> GarbageList {
    let state = &mut *state;
    let previous_interval = take(&mut state.previous_interval);
    
    swap(&mut state.previous_interval, &mut state.current_interval);

    state.thread_was_quiescent
        .iter_mut()
        .for_each(|(_, flag)| *flag = false);
        
    state.num_remaining = state.thread_was_quiescent.len();
    previous_interval
}

pub struct Gc {
    state: Arc<Mutex<GcState>>,
}

struct GcState {
    thread_was_quiescent: HashMap<ThreadId, bool>,
    current_interval: GarbageList,
    previous_interval: GarbageList,
    num_remaining: usize,
}

impl Gc {
    pub fn new() -> Self {
        let state = GcState {
            thread_was_quiescent: HashMap::new(),
            current_interval: Vec::new(),
            previous_interval: Vec::new(),
            num_remaining: 0,
        };

        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn defer(&self, f: impl FnOnce() + 'static) {
        let mut state = self.state.lock().unwrap();
        let f = Box::new(f);
        state.current_interval.push(f);
    }

    pub fn register_thread(&self) {
        let mut state = self.state.lock().unwrap();
        let id = id();
        state.thread_was_quiescent.insert(id, false);
        state.num_remaining += 1;
    }

    pub fn unregister_thread(&self) {
        let mut state = self.state.lock().unwrap();
        let id = id();
        state.thread_was_quiescent.remove(&id);
        state.num_remaining -= 1;
        
        if state.num_remaining == 0 {
            collect(state)
                .into_iter()
                .for_each(|callback| callback());
        }
    }

    pub fn on_quiescent_state(&self) {
        let mut state = self.state.lock().unwrap();
        let id = id();
        let flag = state.thread_was_quiescent.get_mut(&id).unwrap();

        if !*flag {
            *flag = true;
            state.num_remaining -= 1;

            if state.num_remaining == 0 {
                collect(state)
                    .into_iter()
                    .for_each(|callback| callback());
            }
        }
    }
}

impl Drop for Gc {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        
        take(&mut state.previous_interval)
            .into_iter()
            .for_each(|callback| callback());

        take(&mut state.current_interval)
            .into_iter()
            .for_each(|callback| callback());
    }
}
