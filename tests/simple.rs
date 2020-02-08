use dashmap::DashMap;

#[test]
fn insert_once() {
    let map = DashMap::with_capacity(256);
    map.insert(3i32, 6i32);
}
