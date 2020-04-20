fn main() {
    tracing_subscriber::fmt::init();
    exchange(4).run::<DashMapTable<u64>>();
}

use std::sync::Arc;
use bustle::*;
use dashmap::DashMap;

fn ex_mix() -> Mix {
    Mix {
        read: 5,
        insert: 45,
        remove: 45,
        update: 5,
        upsert: 0,
    }
}

fn exchange(n: usize) -> Workload {
    *Workload::new(n, ex_mix())
        .initial_capacity_log2(24)
        .prefill_fraction(0.6)
        .operations(200.0)
}


#[derive(Clone)]
pub struct DashMapTable<K>(Arc<DashMap<K, u32>>);

impl<K> Collection for DashMapTable<K>
where
    K: Send + Sync + From<u64> + Copy + 'static + std::hash::Hash + Eq + std::fmt::Debug,
{
    type Handle = Self;
    fn with_capacity(capacity: usize) -> Self {
        Self(Arc::new(DashMap::with_capacity(capacity)))
    }

    fn pin(&self) -> Self::Handle {
        self.clone()
    }
}

impl<K> CollectionHandle for DashMapTable<K>
where
    K: Send + Sync + From<u64> + Copy + 'static + std::hash::Hash + Eq + std::fmt::Debug,
{
    type Key = K;

    fn get(&mut self, key: &Self::Key) -> bool {
        self.0.extract(key, |_, _| ()).is_some()
    }

    fn insert(&mut self, key: &Self::Key) -> bool {
        self.0.insert(*key, 0) == false
    }

    fn remove(&mut self, key: &Self::Key) -> bool {
        self.0.remove(key)
    }

    fn update(&mut self, key: &Self::Key) -> bool {
        self.0.update(key, |_, v| v + 1)
    }
}
