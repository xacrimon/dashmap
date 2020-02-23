use dashmap::DashMap;

#[test]
fn insert_once() {
    let map = DashMap::with_capacity(256);
    map.insert(3i32, 6i32);
}

#[test]
fn insert_many() {
    const ITER: i32 = 1024 * 1024;
    let map = DashMap::with_capacity(ITER as usize);

    for i in 0..ITER {
        map.insert(i, i + 7);
    }
}
