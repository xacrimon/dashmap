#![forbid(unsafe_code)]

mod util;
pub mod mapref;
pub mod query;

use ahash::ABuildHasher;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use futures::future::{Future, FutureExt};
use hashbrown::HashMap;
use owning_ref::{OwningRef, OwningRefMut};
use std::borrow::Borrow;
use std::convert::TryInto;
use std::hash::{BuildHasher, Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use query::DashMapQuery;

pub struct DashMap<K, V>
where
    K: Eq + Hash,
{
    ncb: usize,
    shards: Box<[RwLock<HashMap<K, V>>]>,
    hash_builder: ABuildHasher,
}

impl<'a, K: 'a + Eq + Hash, V: 'a> DashMap<K, V> {
    pub(crate) fn shards(&'a self) -> &'a Box<[RwLock<HashMap<K, V>>]> {
        &self.shards
    }

    pub(crate) fn determine_map<Q>(&self, key: &Q) -> usize
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut hash_state = self.hash_builder.build_hasher();
        key.hash(&mut hash_state);

        let hash = hash_state.finish();
        let shift = util::ptr_size_bits() - self.ncb;

        (hash >> shift) as usize
    }

    pub fn new() -> Self {
        let shard_amount = (num_cpus::get() * 4).next_power_of_two();
        let shift = (shard_amount as f32).log2() as usize;
        let shards = (0..shard_amount).map(|_| RwLock::new(HashMap::new())).collect::<Vec<_>>().into_boxed_slice();

        Self {
            ncb: shift,
            shards,
            hash_builder: ABuildHasher::new(),
        }
    }

    pub fn query(&'a self) -> DashMapQuery<'a, K, V> {
        DashMapQuery::new(&self)
    }

    #[test]
    fn match_debug() {
        let map = DashMap::default();
        map.insert(1i32, 2i32);
        map.insert(3i32, 6i32);

        let choices = [
            "{1: 2, 3: 6}",
            "{3: 6, 1: 2}",
        ];

        let map_debug = format!("{:?}", map);

        for choice in &choices {
            if map_debug == *choice { return }
        }

        panic!("no match\n{}", map_debug);
    }
}
