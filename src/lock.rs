#[cfg(not(feature = "shuttle"))]
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(feature = "shuttle"))]
use parking_lot_core::{park, unpark_all, unpark_one, ParkToken, SpinWait, UnparkToken};

#[cfg(feature = "shuttle")]
use self::shuttle_parking::{park, unpark_all, unpark_one, ParkToken, SpinWait, UnparkToken};
#[cfg(feature = "shuttle")]
use shuttle::sync::atomic::{AtomicUsize, Ordering};

pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
pub(crate) type RwLockReadGuardDetached<'a> = crate::util::RwLockReadGuardDetached<'a, RawRwLock>;
pub(crate) type RwLockWriteGuardDetached<'a> = crate::util::RwLockWriteGuardDetached<'a, RawRwLock>;

const READERS_PARKED: usize = 0b0001;
const WRITERS_PARKED: usize = 0b0010;
const ONE_READER: usize = 0b0100;
const ONE_WRITER: usize = !(READERS_PARKED | WRITERS_PARKED);

pub struct RawRwLock {
    state: AtomicUsize,
}

unsafe impl lock_api::RawRwLock for RawRwLock {
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self {
        state: AtomicUsize::new(0),
    };

    type GuardMarker = lock_api::GuardSend;

    #[inline]
    fn try_lock_exclusive(&self) -> bool {
        self.state
            .compare_exchange(0, ONE_WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    #[inline]
    fn lock_exclusive(&self) {
        if self
            .state
            .compare_exchange_weak(0, ONE_WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.lock_exclusive_slow();
        }
    }

    #[inline]
    unsafe fn unlock_exclusive(&self) {
        if self
            .state
            .compare_exchange(ONE_WRITER, 0, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            self.unlock_exclusive_slow();
        }
    }

    #[inline]
    fn try_lock_shared(&self) -> bool {
        self.try_lock_shared_fast() || self.try_lock_shared_slow()
    }

    #[inline]
    fn lock_shared(&self) {
        if !self.try_lock_shared_fast() {
            self.lock_shared_slow();
        }
    }

    #[inline]
    unsafe fn unlock_shared(&self) {
        let state = self.state.fetch_sub(ONE_READER, Ordering::Release);

        if state == (ONE_READER | WRITERS_PARKED) {
            self.unlock_shared_slow();
        }
    }
}

unsafe impl lock_api::RawRwLockDowngrade for RawRwLock {
    #[inline]
    unsafe fn downgrade(&self) {
        let state = self
            .state
            .fetch_and(ONE_READER | WRITERS_PARKED, Ordering::Release);
        if state & READERS_PARKED != 0 {
            unpark_all((self as *const _ as usize) + 1, UnparkToken(0));
        }
    }
}

impl RawRwLock {
    #[cold]
    fn lock_exclusive_slow(&self) {
        let mut acquire_with = 0;
        loop {
            let mut spin = SpinWait::new();
            let mut state = self.state.load(Ordering::Relaxed);

            loop {
                while state & ONE_WRITER == 0 {
                    match self.state.compare_exchange_weak(
                        state,
                        state | ONE_WRITER | acquire_with,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => return,
                        Err(e) => state = e,
                    }
                }

                if state & WRITERS_PARKED == 0 {
                    if spin.spin() {
                        state = self.state.load(Ordering::Relaxed);
                        continue;
                    }

                    if let Err(e) = self.state.compare_exchange_weak(
                        state,
                        state | WRITERS_PARKED,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        state = e;
                        continue;
                    }
                }

                let _ = unsafe {
                    park(
                        self as *const _ as usize,
                        || {
                            let state = self.state.load(Ordering::Relaxed);
                            (state & ONE_WRITER != 0) && (state & WRITERS_PARKED != 0)
                        },
                        || {},
                        |_, _| {},
                        ParkToken(0),
                        None,
                    )
                };

                acquire_with = WRITERS_PARKED;
                break;
            }
        }
    }

    #[cold]
    fn unlock_exclusive_slow(&self) {
        let state = self.state.load(Ordering::Relaxed);
        assert_eq!(state & ONE_WRITER, ONE_WRITER);

        let mut parked = state & (READERS_PARKED | WRITERS_PARKED);
        assert_ne!(parked, 0);

        if parked != (READERS_PARKED | WRITERS_PARKED) {
            if let Err(new_state) =
                self.state
                    .compare_exchange(state, 0, Ordering::Release, Ordering::Relaxed)
            {
                assert_eq!(new_state, ONE_WRITER | READERS_PARKED | WRITERS_PARKED);
                parked = READERS_PARKED | WRITERS_PARKED;
            }
        }

        if parked == (READERS_PARKED | WRITERS_PARKED) {
            self.state.store(WRITERS_PARKED, Ordering::Release);
            parked = READERS_PARKED;
        }

        if parked == READERS_PARKED {
            return unsafe {
                unpark_all((self as *const _ as usize) + 1, UnparkToken(0));
            };
        }

        assert_eq!(parked, WRITERS_PARKED);
        unsafe {
            unpark_one(self as *const _ as usize, |_| UnparkToken(0));
        }
    }

    #[inline(always)]
    fn try_lock_shared_fast(&self) -> bool {
        let state = self.state.load(Ordering::Relaxed);

        if let Some(new_state) = state.checked_add(ONE_READER) {
            if new_state & ONE_WRITER != ONE_WRITER {
                return self
                    .state
                    .compare_exchange_weak(state, new_state, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok();
            }
        }

        false
    }

    #[cold]
    fn try_lock_shared_slow(&self) -> bool {
        let mut state = self.state.load(Ordering::Relaxed);

        while let Some(new_state) = state.checked_add(ONE_READER) {
            if new_state & ONE_WRITER == ONE_WRITER {
                break;
            }

            match self.state.compare_exchange_weak(
                state,
                new_state,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(e) => state = e,
            }
        }

        false
    }

    #[cold]
    fn lock_shared_slow(&self) {
        loop {
            let mut spin = SpinWait::new();
            let mut state = self.state.load(Ordering::Relaxed);

            loop {
                let mut backoff = SpinWait::new();
                while let Some(new_state) = state.checked_add(ONE_READER) {
                    assert_ne!(
                        new_state & ONE_WRITER,
                        ONE_WRITER,
                        "reader count overflowed",
                    );

                    if self
                        .state
                        .compare_exchange_weak(
                            state,
                            new_state,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        return;
                    }

                    backoff.spin_no_yield();
                    state = self.state.load(Ordering::Relaxed);
                }

                if state & READERS_PARKED == 0 {
                    if spin.spin() {
                        state = self.state.load(Ordering::Relaxed);
                        continue;
                    }

                    if let Err(e) = self.state.compare_exchange_weak(
                        state,
                        state | READERS_PARKED,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        state = e;
                        continue;
                    }
                }

                let _ = unsafe {
                    park(
                        (self as *const _ as usize) + 1,
                        || {
                            let state = self.state.load(Ordering::Relaxed);
                            (state & ONE_WRITER == ONE_WRITER) && (state & READERS_PARKED != 0)
                        },
                        || {},
                        |_, _| {},
                        ParkToken(0),
                        None,
                    )
                };

                break;
            }
        }
    }

    #[cold]
    fn unlock_shared_slow(&self) {
        if self
            .state
            .compare_exchange(WRITERS_PARKED, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            unsafe {
                unpark_one(self as *const _ as usize, |_| UnparkToken(0));
            }
        }
    }
}

#[cfg(feature = "shuttle")]
mod shuttle_parking {
    use shuttle::sync::Mutex;
    use shuttle::thread::{self, Thread, ThreadId};
    use std::collections::{HashMap, VecDeque};
    use std::sync::OnceLock;
    use std::time::Instant;

    #[allow(dead_code)]
    pub struct ParkToken(pub usize);
    #[allow(dead_code)]
    pub struct UnparkToken(pub usize);

    pub struct ParkResult;
    pub struct UnparkResult;

    pub struct SpinWait {
        counter: u32,
    }

    type WaitQueues = HashMap<usize, VecDeque<Thread>>;

    fn wait_queues() -> &'static Mutex<WaitQueues> {
        static WAIT_QUEUES: OnceLock<Mutex<WaitQueues>> = OnceLock::new();
        WAIT_QUEUES.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn lock_wait_queues() -> shuttle::sync::MutexGuard<'static, WaitQueues> {
        wait_queues()
            .lock()
            .expect("Shuttle wait queue mutex poisoned")
    }

    fn remove_waiter(wait_queues: &mut WaitQueues, key: usize, thread_id: ThreadId) {
        let mut remove_key = false;

        if let Some(queue) = wait_queues.get_mut(&key) {
            if let Some(index) = queue.iter().position(|thread| thread.id() == thread_id) {
                queue.remove(index);
            }

            remove_key = queue.is_empty();
        }

        if remove_key {
            wait_queues.remove(&key);
        }
    }

    impl SpinWait {
        #[inline]
        pub fn new() -> Self {
            Self { counter: 0 }
        }

        #[inline]
        pub fn spin(&mut self) -> bool {
            self.counter += 1;
            if self.counter <= 10 {
                shuttle::thread::yield_now();
                true
            } else {
                false
            }
        }

        #[inline]
        pub fn spin_no_yield(&mut self) {
            shuttle::thread::yield_now();
        }
    }

    pub unsafe fn park(
        key: usize,
        validate: impl FnOnce() -> bool,
        before_sleep: impl FnOnce(),
        _timed_out: impl FnOnce(usize, bool),
        _park_token: ParkToken,
        _timeout: Option<Instant>,
    ) -> ParkResult {
        let current = thread::current();

        {
            let mut wait_queues = lock_wait_queues();
            if !validate() {
                return ParkResult;
            }

            wait_queues
                .entry(key)
                .or_default()
                .push_back(current.clone());
            before_sleep();
        }

        thread::park();

        let mut wait_queues = lock_wait_queues();
        remove_waiter(&mut wait_queues, key, current.id());

        ParkResult
    }

    pub unsafe fn unpark_one(key: usize, callback: impl FnOnce(UnparkResult) -> UnparkToken) {
        let waiter = {
            let mut wait_queues = lock_wait_queues();
            let waiter = wait_queues.get_mut(&key).and_then(VecDeque::pop_front);

            if wait_queues.get(&key).is_some_and(VecDeque::is_empty) {
                wait_queues.remove(&key);
            }

            waiter
        };

        let _ = callback(UnparkResult);

        if let Some(waiter) = waiter {
            waiter.unpark();
        }
    }

    pub unsafe fn unpark_all(key: usize, _token: UnparkToken) {
        let waiters = {
            let mut wait_queues = lock_wait_queues();
            wait_queues
                .remove(&key)
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>()
        };

        // Wake waiters after dropping the queue mutex so they can immediately
        // re-register themselves if the outer lock algorithm loops.
        for waiter in waiters {
            waiter.unpark();
        }
    }
}
