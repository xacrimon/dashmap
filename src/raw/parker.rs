use std::collections::HashMap;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::{self, Thread};

// A simpler thread parker.
//
// This parker is rarely used and relatively naive. Ideally this would just use `futex`
// but the hashmap needs to park on tagged pointer state, and mixed-sized atomic accesses
// are questionable.
//
// The parker implementation may be sharded and use intrusive lists if it is found to be
// a bottleneck.
#[derive(Default)]
pub struct Parker {
    pending: AtomicU64,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    count: u64,
    threads: HashMap<usize, HashMap<u64, Thread>>,
}

impl Parker {
    // Block the current thread until the park condition is false.
    pub fn park<T>(&self, atomic: &AtomicPtr<T>, should_park: impl Fn(*mut T) -> bool) {
        let key = atomic as *const _ as usize;

        loop {
            // Announce our thread.
            //
            // This must be done before inserting our thread to prevent
            // incorrect decrements if we are unparked in-between inserting
            // the thread and incrementing the counter.
            self.pending.fetch_add(1, Ordering::SeqCst);

            // Insert our thread into the parker.
            let id = {
                let state = &mut *self.state.lock().unwrap();
                state.count += 1;

                let threads = state.threads.entry(key).or_default();
                threads.insert(state.count, thread::current());

                state.count
            };

            // Check the park condition.
            if !should_park(atomic.load(Ordering::SeqCst)) {
                // Don't need to park, remove our thread if it wasn't already unparked.
                let thread = {
                    let mut state = self.state.lock().unwrap();
                    state
                        .threads
                        .get_mut(&key)
                        .and_then(|threads| threads.remove(&id))
                };

                if thread.is_some() {
                    self.pending.fetch_sub(1, Ordering::Relaxed);
                }

                return;
            }

            // Park until we are unparked.
            loop {
                thread::park();

                let mut state = self.state.lock().unwrap();
                if !state
                    .threads
                    .get_mut(&key)
                    .is_some_and(|threads| threads.contains_key(&id))
                {
                    break;
                }
            }

            // Ensure we were unparked for the correct reason.
            if !should_park(atomic.load(Ordering::Acquire)) {
                return;
            }
        }
    }

    // Unpark all threads waiting on the given atomic.
    //
    // Note that any modifications must be `SeqCst` to be visible to unparked threads.
    pub fn unpark<T>(&self, atomic: &AtomicPtr<T>) {
        let key = atomic as *const _ as usize;

        // Fast-path, no one waiting to be unparked.
        if self.pending.load(Ordering::SeqCst) == 0 {
            return;
        }

        // Remove and unpark any threads waiting on the atomic.
        let threads = {
            let mut state = self.state.lock().unwrap();
            state.threads.remove(&key)
        };

        if let Some(threads) = threads {
            let unparked = threads.len() as u64;
            self.pending.fetch_sub(unparked, Ordering::Relaxed);

            for (_, thread) in threads {
                thread.unpark();
            }
        }
    }
}
