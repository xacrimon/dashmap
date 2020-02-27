use std::sync::{Mutex, Arc};
use std::mem::{swap, take};
use std::thread::{ThreadId, current};
use std::collections::HashMap;

type GarbageList = Vec<Box<dyn FnOnce()>>;

fn id() -> ThreadId {
    current().id()
}

pub struct Gc {
    state: Arc<Mutex<GcState>>,
}

impl Gc {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(GcState::new())),
        }
    }

    pub fn register_thread(&self) {
        self.state.lock().unwrap().register_thread();
    }

    pub fn unregister_thread(&self) {
        self.state.lock().unwrap().unregister_thread();
    }

    pub fn on_quiescent_state(&self) {
        self.state.lock().unwrap().on_quiescent_state();
    }
}

struct GcState {
    thread_was_quiescent: HashMap<ThreadId, bool>,
    current_interval: GarbageList,
    previous_interval: GarbageList,
    num_remaining: usize,
}

impl GcState {
    fn new() -> Self {
        Self {
            thread_was_quiescent: HashMap::new(),
            current_interval: Vec::new(),
            previous_interval: Vec::new(),
            num_remaining: 0,
        }
    }

    fn collect(&mut self) {
        let previous_interval = take(&mut self.previous_interval);

        previous_interval
            .into_iter()
            .for_each(|callback| callback());
        
        swap(&mut self.previous_interval, &mut self.current_interval);

        self.thread_was_quiescent
            .iter_mut()
            .for_each(|(_, flag)| *flag = false);
        
        self.num_remaining = self.thread_was_quiescent.len();
    }

    fn register_thread(&mut self) {
        let id = id();
        self.thread_was_quiescent.insert(id, false);
        self.num_remaining += 1;
    }

    fn unregister_thread(&mut self) {
        let id = id();
        self.thread_was_quiescent.remove(&id);
        self.num_remaining -= 1;
        
        if self.num_remaining == 0 {
            self.collect();
        }
    }

    fn add_callback(&mut self, callback: Box<dyn FnOnce()>) {
        self.current_interval.push(callback);
    }

    fn on_quiescent_state(&mut self) {
        let id = id();
        let flag = self.thread_was_quiescent.get_mut(&id).unwrap();

        if !*flag {
            *flag = true;
            self.num_remaining -= 1;

            if self.num_remaining == 0 {
                self.collect();
            }
        }
    }
}
