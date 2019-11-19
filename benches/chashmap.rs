use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use rayon::prelude::*;
use chashmap::CHashMap;

const ITER: u64 = 8 * 1024;

fn task_insert_chashmap_u64_u64() -> CHashMap<u64, u64> {
    let map = CHashMap::new();
    (0..ITER).into_par_iter().for_each(|i| {
        map.insert(i, i + 7);
    });
    map
}

fn insert_chashmap_u64_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_chashmap_u64_u64");
    group.throughput(Throughput::Elements(ITER as u64));
    let max = num_cpus::get();

    for threads in 1..=max {
        group.bench_with_input(BenchmarkId::from_parameter(threads), &threads, |b, &threads| {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            pool.install(|| b.iter(|| task_insert_chashmap_u64_u64()));
        });
    }

    group.finish();
}

criterion_group!(benches, insert_chashmap_u64_u64);
criterion_main!(benches);
