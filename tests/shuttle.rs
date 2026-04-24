use dashmap::DashMap;
use shuttle::sync::atomic::{AtomicBool, Ordering};
use shuttle::{check_random, thread};
use std::sync::Arc;

fn test_map() -> DashMap<i32, &'static str> {
    DashMap::with_shard_amount(4)
}

#[test]
fn concurrent_insert_and_read() {
    check_random(
        || {
            let map = Arc::new(test_map());
            let map2 = Arc::clone(&map);

            let t1 = thread::spawn(move || {
                map2.insert(1, "a");
            });

            map.insert(2, "b");
            t1.join().expect("insert thread panicked");

            assert_eq!(map.len(), 2);
        },
        1000,
    );
}

#[test]
fn concurrent_insert_and_remove() {
    check_random(
        || {
            let map = Arc::new(test_map());
            map.insert(1, "initial");

            let map2 = Arc::clone(&map);
            let t1 = thread::spawn(move || {
                map2.remove(&1);
            });

            let map3 = Arc::clone(&map);
            let t2 = thread::spawn(move || {
                map3.insert(1, "replaced");
            });

            t1.join().expect("remove thread panicked");
            t2.join().expect("insert thread panicked");

            match map.get(&1) {
                Some(value) => assert_eq!(*value, "replaced"),
                None => {}
            };
        },
        1000,
    );
}

#[test]
fn concurrent_entry_api() {
    check_random(
        || {
            let map = Arc::new(test_map());
            let map2 = Arc::clone(&map);

            let t1 = thread::spawn(move || {
                map2.entry(1).or_insert("first");
            });

            map.entry(1).or_insert("second");
            t1.join().expect("entry thread panicked");

            let value = map.get(&1).unwrap();
            assert!(*value == "first" || *value == "second");
        },
        1000,
    );
}

#[test]
fn writer_waits_for_active_reader() {
    check_random(
        || {
            let map = Arc::new(test_map());
            map.insert(1, "initial");

            let reader_map = Arc::clone(&map);
            let writer_map = Arc::clone(&map);
            let reader_ready = Arc::new(AtomicBool::new(false));

            let reader_ready_for_reader = Arc::clone(&reader_ready);
            let reader = thread::spawn(move || {
                let value = reader_map.get(&1).expect("missing value");
                reader_ready_for_reader.store(true, Ordering::Release);
                thread::yield_now();
                assert_eq!(*value, "initial");
            });

            let reader_ready_for_writer = Arc::clone(&reader_ready);
            let writer = thread::spawn(move || {
                while !reader_ready_for_writer.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                writer_map.insert(1, "updated");
            });

            reader.join().expect("reader thread panicked");
            writer.join().expect("writer thread panicked");

            assert_eq!(*map.get(&1).expect("missing updated value"), "updated");
        },
        1000,
    );
}

#[test]
fn downgrade_wakes_waiting_reader() {
    check_random(
        || {
            let map = Arc::new(test_map());
            map.insert(1, "initial");

            let writer_map = Arc::clone(&map);
            let reader_map = Arc::clone(&map);

            let writer = thread::spawn(move || {
                let write_ref = writer_map.get_mut(&1).expect("missing value");
                thread::yield_now();

                let read_ref = write_ref.downgrade();
                thread::yield_now();
                assert_eq!(*read_ref, "initial");
            });

            let reader = thread::spawn(move || {
                let value = reader_map.get(&1).expect("missing value");
                assert_eq!(*value, "initial");
            });

            writer.join().expect("writer thread panicked");
            reader.join().expect("reader thread panicked");
        },
        1000,
    );
}
