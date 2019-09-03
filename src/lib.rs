// TO-DO: entry api
//        api results instead
//        coarse transactions
//        shortcuts api
//        fix deadlock
//        optimizations
//        fine grained transactions
//        optimizations
//        useful traits
//        tests
//        docs
//        WHEN STABLE ASYNC AWAIT: async apis

pub mod iter;
pub mod mapref;
pub mod query;
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
        let shard_amount = (num_cpus::get() * 16).next_power_of_two();
        let shift = (shard_amount as f32).log2() as usize;
        let shards = (0..shard_amount)
            .map(|_| RwLock::new(HashMap::new()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            ncb: shift,
            shards,
            hash_builder: ABuildHasher::new(),
        }
    }

    pub fn query(&'a self) -> Query<'a, K, V> {
        Query::new(&self)
    }
}
