use std::hash::Hash;
use dashmap_shard::HashMap;
use fxhash::FxBuildHasher;
use parking_lot::RwLock;
use std::borrow::Borrow;
use super::DashMap;

pub trait MapProvider<'a, K: 'a + Eq + Hash, V: 'a> {
    fn shards(&'a self) -> &'a [RwLock<HashMap<K, V, FxBuildHasher>>];
    fn determine_map<Q>(&self, key: &Q) -> (usize, u64)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;
}

impl<'a, K: 'a + Eq + Hash, V: 'a> MapProvider<'a, K, V> for DashMap<K, V> {
    fn shards(&'a self) -> &'a [RwLock<HashMap<K, V, FxBuildHasher>>] {
        self._shards()
    }

    fn determine_map<Q>(&self, key: &Q) -> (usize, u64)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized
    {
        self._determine_map(key)
    }
}
