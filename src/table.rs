use super::u128::AtomicU128;
use std::borrow::Borrow;
use std::cell::UnsafeCell;
use std::hash::{BuildHasher, Hash, Hasher};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const TOMBSTONE_BIT: u64 = 1 << 63;
const ALLOCATED_BIT: u64 = 1 << 62;
const POINTER_MASK: u64 = 0x3FFFFFFFFFFFFFFF;

fn hash<S, K>(hasher: &S, key: &K) -> u64
where
    S: BuildHasher,
    K: Hash,
{
    let mut hasher = hasher.build_hasher();
    key.hash(&mut hasher);
    hasher.finish()
}

struct Slot<K, V> {
    data: AtomicU64,
    pair: UnsafeCell<MaybeUninit<(K, V)>>,
}

pub struct Table<K, V, S> {
    hash: Arc<S>,
    slots: Box<[Slot<K, V>]>,
    mask: usize,
}

impl<K, V, S> Table<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    pub fn new(hasher: Arc<S>, capacity: usize) -> Self {
        debug_assert!(capacity.is_power_of_two());
        let slots = (0..capacity)
            .map(|_| Slot {
                data: AtomicU64::new(0),
                pair: UnsafeCell::new(MaybeUninit::uninit()),
            })
            .collect::<Vec<_>>();

        Table {
            hash: hasher,
            slots: slots.into_boxed_slice(),
            mask: capacity - 1,
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<*mut (K, V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        let hash = hash(&*self.hash, key);
        let mut idx = hash as usize & self.mask;
        let mut i = 0;

        loop {
            let slot = &self.slots[idx];
            let data = slot.data.load(Ordering::Relaxed);
            let ptr = (data & POINTER_MASK) as *mut (K, V);
            if !ptr.is_null() {
                let stored = unsafe { (*ptr).0.borrow() };
                if stored == key {
                    return Some(ptr);
                }
            } else if data & TOMBSTONE_BIT != TOMBSTONE_BIT || i > self.mask {
                return None;
            }

            idx = (idx + 1) & self.mask;
            i += 1;
        }
    }
}
