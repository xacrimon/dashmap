use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use rayon::prelude::*;
use cht::HashMap;

const ITER: u64 = 8 * 1024;

fn task_insert_cht_u64_u64() -> HashMap<u64, u64> {
    let map = HashMap::new();
    (0..ITER).into_par_iter().for_each(|i| {
        map.insert(i, i + 7);
    });
    map
}

fn insert_cht_u64_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_cht_u64_u64");
    group.throughput(Throughput::Elements(ITER as u64));
    let max = num_cpus::get();

    for threads in 1..=max {
        group.bench_with_input(BenchmarkId::from_parameter(threads), &threads, |b, &threads| {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            pool.install(|| b.iter(|| task_insert_cht_u64_u64()));
        });
    }

    group.finish();
}

criterion_group!(benches, insert_cht_u64_u64);
criterion_main!(benches);
