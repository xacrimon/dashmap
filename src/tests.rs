use super::{DashMap, ExecutableQuery};

#[test]
fn insert_and_get() {
    let map: DashMap<i32, i32> = DashMap::new();
    map.query().insert(19, 420).sync().exec();
    assert_eq!(*map.query().get(&19).sync().exec().unwrap(), 420);
}

#[test]
fn insert_and_remove() {
    let map: DashMap<i32, i32> = DashMap::new();
    map.query().insert(19, 420).sync().exec();
    assert_eq!(map.query().remove(&19).sync().exec(), Some((19, 420)));
}

#[test]
fn insert_iter_count() {
    let map: DashMap<i32, i32> = DashMap::new();
    map.query().insert(19, 420).sync().exec();
    map.query().insert(13, 420).sync().exec();
    assert_eq!(map.query().iter().exec().count(), 2);
}
