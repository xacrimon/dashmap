pub mod iter;
pub mod mapref;
mod util;

use dashmap_shard::HashMap;
use fxhash::FxBuildHasher;
use iter::{Iter, IterMut};
use mapref::entry::{Entry, OccupiedEntry, VacantEntry};
use mapref::one::{Ref, RefMut};
use parking_lot::RwLock;
use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};

fn shard_amount() -> usize {
    (num_cpus::get() * 4).next_power_of_two()
}

fn ncb(shard_amount: usize) -> usize {
    (shard_amount as f32).log2() as usize
}

#[derive(Default)]
pub struct DashMap<K, V>
where
    K: Eq + Hash,
{
    ncb: usize,
    shards: Box<[RwLock<HashMap<K, V, FxBuildHasher>>]>,
    hash_builder: FxBuildHasher,
}

impl<'a, K: 'a + Eq + Hash, V: 'a> DashMap<K, V> {
    pub fn new() -> Self {
        let shard_amount = shard_amount();
        let shards = (0..shard_amount)
            .map(|_| RwLock::new(HashMap::with_hasher(FxBuildHasher::default())))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            ncb: ncb(shard_amount),
            shards,
            hash_builder: FxBuildHasher::default(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let shard_amount = shard_amount();
        let cps = capacity / shard_amount;
        let shards = (0..shard_amount)
            .map(|_| {
                RwLock::new(HashMap::with_capacity_and_hasher(
                    cps,
                    FxBuildHasher::default(),
                ))
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            ncb: ncb(shard_amount),
            shards,
            hash_builder: FxBuildHasher::default(),
        }
    }

    pub fn shards(&'a self) -> &'a [RwLock<HashMap<K, V, FxBuildHasher>>] {
        &self.shards
    }

    pub fn determine_map<Q>(&self, key: &Q) -> (usize, u64)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut hash_state = self.hash_builder.build_hasher();
        key.hash(&mut hash_state);

        let hash = hash_state.finish();
        let shift = util::ptr_size_bits() - self.ncb;

        ((hash >> shift) as usize, hash)
    }

    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let (shard, hash) = self.determine_map(&key);
        let mut shard = self.shards[shard].write();
        shard.insert_with_hash_nocheck(key, value, hash)
    }

    pub fn remove<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (shard, _) = self.determine_map(key);
        let mut shard = self.shards[shard].write();
        shard.remove_entry(key)
    }

    pub fn iter(&'a self) -> Iter<'a, K, V> {
        Iter::new(self)
    }

    pub fn iter_mut(&'a self) -> IterMut<'a, K, V> {
        IterMut::new(self)
    }

    pub fn get<Q>(&'a self, key: &Q) -> Option<Ref<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (shard, _) = self.determine_map(key);
        let shard = self.shards[shard].read();
        if let Some((kptr, vptr)) = shard.get_key_value(key) {
            unsafe {
                let kptr = util::change_lifetime_const(kptr);
                let vptr = util::change_lifetime_const(vptr);
                Some(Ref::new(shard, kptr, vptr))
            }
        } else {
            None
        }
    }

    pub fn get_mut<Q>(&'a self, key: &Q) -> Option<RefMut<'a, K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (shard, _) = self.determine_map(key);
        let shard = self.shards[shard].write();
        if let Some((kptr, vptr)) = shard.get_key_value(key) {
            unsafe {
                let kptr = util::change_lifetime_const(kptr);
                let vptr = util::change_lifetime_mut(util::to_mut(vptr));
                Some(RefMut::new(shard, kptr, vptr))
            }
        } else {
            None
        }
    }

    pub fn shrink_to_fit(&self) {
        self.shards.iter().for_each(|s| s.write().shrink_to_fit());
    }

    pub fn retain(&self, mut f: impl FnMut(&K, &mut V) -> bool) {
        self.shards.iter().for_each(|s| s.write().retain(&mut f));
    }

    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.shards.iter().for_each(|s| s.write().clear());
    }

    pub fn capacity(&self) -> usize {
        self.shards.iter().map(|s| s.read().capacity()).sum()
    }

    pub fn alter<Q>(&self, key: &Q, f: impl FnOnce(&K, V) -> V)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if let Some(mut r) = self.get_mut(key) {
            util::map_in_place_2(r.pair_mut(), f);
        }
    }

    pub fn alter_all(&self, mut f: impl FnMut(&K, V) -> V) {
        self.shards.iter().for_each(|s| {
            s.write()
                .iter_mut()
                .for_each(|pair| util::map_in_place_2(pair, &mut f));
        });
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(key).is_some()
    }

    pub fn entry(&'a self, key: K) -> Entry<'a, K, V> {
        let (shard, _) = self.determine_map(&key);
        let shard = self.shards[shard].write();
        if let Some((kptr, vptr)) = shard.get_key_value(&key) {
            unsafe {
                let kptr = util::change_lifetime_const(kptr);
                let vptr = util::change_lifetime_mut(util::to_mut(vptr));
                Entry::Occupied(OccupiedEntry::new(shard, Some(key), (kptr, vptr)))
            }
        } else {
            Entry::Vacant(VacantEntry::new(shard, key))
        }
    }
}
