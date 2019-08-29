use super::{DashMap, DashMapExecutableQuery};

#[test]
fn insert_and_get() {
    let map: DashMap<i32, i32> = DashMap::new();
    map.query().insert(19, 420).sync().exec();
    assert_eq!(*map.query().get(&19).sync().exec().unwrap(), 420);
}
