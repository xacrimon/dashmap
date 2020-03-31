use bustle::*;
use dashmap::DashMap as DashMapExperimental;
use fxhash::FxBuildHasher;
use std::sync::Arc;

#[derive(Clone)]
pub struct DashMapExperimentalTable<K>(Arc<DashMapExperimental<K, u32, FxBuildHasher>>);

impl<K> Collection for DashMapExperimentalTable<K>
where
    K: Send + Sync + From<u64> + Copy + 'static + std::hash::Hash + Eq + std::fmt::Debug,
{
    type Handle = Self;
    fn with_capacity(capacity: usize) -> Self {
        Self(Arc::new(DashMapExperimental::with_capacity_and_hasher(
            capacity,
            FxBuildHasher::default(),
        )))
    }

    fn pin(&self) -> Self::Handle {
        self.clone()
    }
}

impl<K> CollectionHandle for DashMapExperimentalTable<K>
where
    K: Send + Sync + From<u64> + Copy + 'static + std::hash::Hash + Eq + std::fmt::Debug,
{
    type Key = K;

    fn get(&mut self, key: &Self::Key) -> bool {
        //dbg!(self.0.capacity());
        //dbg!("get");
        let r = self.0.extract(key, |_, _| ()).is_some();
        r
    }

    fn insert(&mut self, key: &Self::Key) -> bool {
        //dbg!("insert, {}", key);
        let r = self.0.insert(*key, 0) == false;
        r
    }

    fn remove(&mut self, key: &Self::Key) -> bool {
        //dbg!("remove");
        let r = self.0.remove(key);
        r
    }

    fn update(&mut self, key: &Self::Key) -> bool {
        //dbg!("update");
        let r = self.0.update(key, &mut |_, v| v + 1);
        r
    }
}

static EXCHANGE_PREFILL: [f64; 10] = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
static EXCHANGE_OPS: f64 = 1.5;

fn main() {
    tracing_subscriber::fmt::init();
    let mut workload = *Workload::new(num_cpus::get(), Mix::read_heavy()).operations(EXCHANGE_OPS);
    dbg!(workload);

    for pfrac in &EXCHANGE_PREFILL {
        let customized = workload.prefill_fraction(*pfrac);
        dbg!(pfrac);
        customized.run::<DashMapExperimentalTable<u64>>();
    }
}
