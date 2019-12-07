use crate::mapref::interface::{RefInterface, RefMutInterface};
use crate::t::Map;
use crate::util;
use crate::DashMap;
use dashmap_shard::HashMap as ShardHashMap;
use fxhash::FxBuildHasher;
use parking_lot::RwLockWriteGuard;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;

fn tt_aq_shared(map: &RefCell<HashMap<u64, BorrowStatus>>, hash: u64) -> bool {
    let mut map = map.borrow_mut();
    if let Some(status) = map.get_mut(&hash) {
        match status {
            BorrowStatus::Shared(c) => {
                *c += 1;
                true
            }
            BorrowStatus::Exclusive => false,
        }
    } else {
        map.insert(hash, BorrowStatus::Shared(1));
        true
    }
}

fn tt_aq_exclusive(map: &RefCell<HashMap<u64, BorrowStatus>>, hash: u64) -> bool {
    let mut map = map.borrow_mut();
    if let Some(_) = map.get(&hash) {
        false
    } else {
        map.insert(hash, BorrowStatus::Exclusive);
        true
    }
}

enum BorrowStatus {
    Shared(usize),
    Exclusive,
}

pub struct Interface<'a, K: Eq + Hash, V> {
    base: &'a DashMap<K, V>,
    iic: RefCell<HashMap<usize, RwLockWriteGuard<'a, ShardHashMap<K, V, FxBuildHasher>>>>,
    borrows: RefCell<HashMap<u64, BorrowStatus>>,
}

impl<'a, K: Eq + Hash, V> Interface<'a, K, V> {
    fn ensure_has(&self, i: usize) {
        let mut iic = self.iic.borrow_mut();
        if !iic.contains_key(&i) {
            let guard = unsafe { self.base._yield_write_shard(i) };
            iic.insert(i, guard);
        }
    }

    pub fn get<Q>(&'a self, key: &Q) -> Option<RefInterface<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (i, hash) = self.base.determine_map(key);
        if tt_aq_shared(&self.borrows, hash) {
            self.ensure_has(i);
            let iic = self.iic.borrow();
            let shard = &iic[&i];
            if let Some((kptr, vptr)) = shard.get_hash_nocheck_key_value(hash, key) {
                unsafe {
                    let kptr = util::change_lifetime_const(kptr);
                    let vptr = util::change_lifetime_const(vptr);
                    Some(RefInterface::new(kptr, vptr))
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_mut<Q>(&'a self, key: &Q) -> Option<RefMutInterface<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (i, hash) = self.base.determine_map(key);
        if tt_aq_exclusive(&self.borrows, hash) {
            self.ensure_has(i);
            let iic = self.iic.borrow();
            let shard = &iic[&i];
            if let Some((kptr, vptr)) = shard.get_hash_nocheck_key_value(hash, key) {
                unsafe {
                    let kptr = util::change_lifetime_const(kptr);
                    let vptr = util::change_lifetime_mut(util::to_mut(vptr));
                    Some(RefMutInterface::new(kptr, vptr))
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}
