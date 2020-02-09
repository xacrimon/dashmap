use dashmap::DashMap;

#[test]
fn insert_once() {
    let map = DashMap::with_capacity(256);
    map.insert(3i32, 6i32);
}

#[test]
fn insert_many() {
    let map = DashMap::with_capacity(256);

    for i in 0..256i32 {
        map.insert(i, i + 7);
    }
}
