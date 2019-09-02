pub mod mapref;
pub mod query;
pub mod iter;
mod tlock;
mod util;

#[cfg(test)]
mod tests;

use ahash::ABuildHasher;
use hashbrown::HashMap;
use parking_lot::RwLock;
pub use query::ExecutableQuery;
use query::Query;
use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};
use tlock::TransactionLock;

pub struct DashMap<K, V>
where
    K: Eq + Hash,
{
    ncb: usize,
    shards: Box<[RwLock<HashMap<K, V>>]>,
    hash_builder: ABuildHasher,
    transaction_lock: TransactionLock,
}

impl<'a, K: 'a + Eq + Hash, V: 'a> DashMap<K, V> {
    pub(crate) fn shards(&'a self) -> &'a Box<[RwLock<HashMap<K, V>>]> {
        &self.shards
    }

    pub(crate) fn transaction_lock(&'a self) -> &'a TransactionLock {
        &self.transaction_lock
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
        let shards = (0..shard_amount)
            .map(|_| RwLock::new(HashMap::new()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            ncb: shift,
            shards,
            hash_builder: ABuildHasher::new(),
            transaction_lock: TransactionLock::new(),
        }
    }

    pub fn query(&'a self) -> Query<'a, K, V> {
        Query::new(&self)
    }

    pub fn transaction<R>(&'a self, f: impl FnOnce(&DashMap<K, V>) -> R) -> R {
        self.transaction_lock.acquire_unique();
        let r = f(&self);
        unsafe { self.transaction_lock.release_unique() }
        r
    }
}
