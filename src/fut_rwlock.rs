use parking_lot::Mutex as RegularMutex;
use parking_lot::RwLock as RegularRwLock;
use parking_lot::RwLockReadGuard as RegularRwLockReadGuard;
use parking_lot::RwLockWriteGuard as RegularRwLockWriteGuard;
use slab::Slab;
use stable_deref_trait::StableDeref;
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

const WAIT_KEY_NONE: usize = std::usize::MAX;

enum Waiter {
    Waiting(Waker),
    Woken,
}

impl Waiter {
    #[inline]
    fn register(&mut self, w: &Waker) {
        match self {
            Waiter::Waiting(waker) if w.will_wake(waker) => {}
            _ => *self = Waiter::Waiting(w.clone()),
        }
    }

    #[inline]
    fn wake(&mut self) {
        match mem::replace(self, Waiter::Woken) {
            Waiter::Waiting(waker) => waker.wake(),
            Waiter::Woken => {}
        }
    }
}

pub struct RwLockReadGuard<'a, T> {
    _inner_guard: Option<RegularRwLockReadGuard<'a, ()>>,
    lock: &'a RwLock<T>,
}

impl<'a, T> Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> Drop for RwLockReadGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        drop(self._inner_guard.take());
        let mut waiters = self.lock.waiters.lock();
        if let Some((_i, waiter)) = waiters.iter_mut().next() {
            waiter.wake();
        }
    }
}

pub struct RwLockWriteGuard<'a, T> {
    _inner_guard: Option<RegularRwLockWriteGuard<'a, ()>>,
    lock: &'a RwLock<T>,
}

impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for RwLockWriteGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        drop(self._inner_guard.take());
        let mut waiters = self.lock.waiters.lock();
        if let Some((_i, waiter)) = waiters.iter_mut().next() {
            waiter.wake();
        }
    }
}

pub struct RwLock<T> {
    lock: RegularRwLock<()>,
    waiters: RegularMutex<Slab<Waiter>>,
    data: UnsafeCell<T>,
}

impl<T> RwLock<T> {
    fn remove_waker(&self, wait_key: usize, wake_another: bool) {
        if wait_key != WAIT_KEY_NONE {
            let mut waiters = self.waiters.lock();
            match waiters.remove(wait_key) {
                Waiter::Waiting(_) => {}
                Waiter::Woken => {
                    // We were awoken, but then dropped before we could
                    // wake up to acquire the lock. Wake up another
                    // waiter.
                    if wake_another {
                        if let Some((_i, waiter)) = waiters.iter_mut().next() {
                            waiter.wake();
                        }
                    }
                }
            }
        }
    }

    pub fn new(data: T) -> Self {
        Self {
            lock: RegularRwLock::new(()),
            waiters: RegularMutex::new(Slab::new()),
            data: UnsafeCell::new(data),
        }
    }

    #[inline]
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        self.lock.try_read().map(|guard| RwLockReadGuard {
            _inner_guard: Some(guard),
            lock: self,
        })
    }

    #[inline]
    pub fn try_read_for(&self, d: Duration) -> Option<RwLockReadGuard<'_, T>> {
        self.lock.try_read_for(d).map(|guard| RwLockReadGuard {
            _inner_guard: Some(guard),
            lock: self,
        })
    }

    #[inline]
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        self.lock.try_write().map(|guard| RwLockWriteGuard {
            _inner_guard: Some(guard),
            lock: self,
        })
    }

    #[inline]
    pub fn try_write_for(&self, d: Duration) -> Option<RwLockWriteGuard<'_, T>> {
        self.lock.try_write_for(d).map(|guard| RwLockWriteGuard {
            _inner_guard: Some(guard),
            lock: self,
        })
    }

    #[inline]
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadGuard {
            _inner_guard: Some(self.lock.read()),
            lock: self,
        }
    }

    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        RwLockWriteGuard {
            _inner_guard: Some(self.lock.write()),
            lock: self,
        }
    }

    #[inline]
    pub fn async_read(&self) -> RwLockReadFuture<'_, T> {
        RwLockReadFuture {
            lock: Some(self),
            wait_key: WAIT_KEY_NONE,
        }
    }

    #[inline]
    pub fn async_write(&self) -> RwLockWriteFuture<'_, T> {
        RwLockWriteFuture {
            lock: Some(self),
            wait_key: WAIT_KEY_NONE,
        }
    }
}

pub struct RwLockReadFuture<'a, T> {
    lock: Option<&'a RwLock<T>>,
    wait_key: usize,
}

impl<'a, T> Drop for RwLockReadFuture<'a, T> {
    #[inline]
    fn drop(&mut self) {
        if let Some(lock) = self.lock {
            lock.remove_waker(self.wait_key, true);
        }
    }
}

impl<'a, T> Future for RwLockReadFuture<'a, T> {
    type Output = RwLockReadGuard<'a, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let lock = self.lock.expect("polled after completion");

        if let Some(guard) = lock.try_read() {
            lock.remove_waker(self.wait_key, false);
            self.lock = None;
            return Poll::Ready(guard);
        }

        {
            let mut waiters = lock.waiters.lock();
            if self.wait_key == WAIT_KEY_NONE {
                self.wait_key = waiters.insert(Waiter::Waiting(cx.waker().clone()));
            } else {
                waiters[self.wait_key].register(cx.waker())
            }
        }

        if let Some(guard) = lock.try_read() {
            lock.remove_waker(self.wait_key, false);
            self.lock = None;
            return Poll::Ready(guard);
        }

        Poll::Pending
    }
}

pub struct RwLockWriteFuture<'a, T> {
    lock: Option<&'a RwLock<T>>,
    wait_key: usize,
}

impl<'a, T> Drop for RwLockWriteFuture<'a, T> {
    #[inline]
    fn drop(&mut self) {
        if let Some(lock) = self.lock {
            lock.remove_waker(self.wait_key, true);
        }
    }
}

impl<'a, T> Future for RwLockWriteFuture<'a, T> {
    type Output = RwLockWriteGuard<'a, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let lock = self.lock.expect("polled after completion");

        if let Some(guard) = lock.try_write() {
            lock.remove_waker(self.wait_key, false);
            self.lock = None;
            return Poll::Ready(guard);
        }

        {
            let mut waiters = lock.waiters.lock();
            if self.wait_key == WAIT_KEY_NONE {
                self.wait_key = waiters.insert(Waiter::Waiting(cx.waker().clone()));
            } else {
                waiters[self.wait_key].register(cx.waker())
            }
        }

        if let Some(guard) = lock.try_write() {
            lock.remove_waker(self.wait_key, false);
            self.lock = None;
            return Poll::Ready(guard);
        }

        Poll::Pending
    }
}

unsafe impl<T: Send> Send for RwLock<T> {}
unsafe impl<T: Sync> Sync for RwLock<T> {}

unsafe impl<T: Send> Send for RwLockReadFuture<'_, T> {}
unsafe impl<T: Send> Sync for RwLockReadFuture<'_, T> {}

unsafe impl<T: Send> Send for RwLockWriteFuture<'_, T> {}
unsafe impl<T: Send> Sync for RwLockWriteFuture<'_, T> {}

unsafe impl<T: Send> Send for RwLockReadGuard<'_, T> {}
unsafe impl<T: Sync> Sync for RwLockReadGuard<'_, T> {}

unsafe impl<T: Send> Send for RwLockWriteGuard<'_, T> {}
unsafe impl<T: Sync> Sync for RwLockWriteGuard<'_, T> {}

unsafe impl<T> StableDeref for RwLockReadGuard<'_, T> {}
unsafe impl<T> StableDeref for RwLockWriteGuard<'_, T> {}
