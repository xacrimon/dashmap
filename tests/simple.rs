use dashmap::DashMap;

#[test]
fn insert_get() {
    const ITER: i32 = 1024;
    let map = DashMap::new();

    for i in 0..ITER {
        map.insert(i, i + 7);
    }

    for i in 0..ITER {
        assert_eq!(*map.get(&i).unwrap(), i + 7);
    }
}

#[test]
fn insert_extract() {
    const ITER: i32 = 1024;
    let map = DashMap::with_capacity(ITER as usize);

    for i in 0..ITER {
        map.insert(i, i + 7);
    }

    for i in 0..ITER {
        let v = map.extract(&i, |_, v| *v).unwrap();
        assert_eq!(v, i + 7);
    }
}

#[test]
fn insert_remove() {
    const ITER: i32 = 1024;
    let map = DashMap::with_capacity(ITER as usize);

    for i in 0..ITER {
        map.insert(i, i + 7);
    }

    for i in 0..ITER {
        dbg!(i);
        assert!(map.remove(&i));
    }
}
