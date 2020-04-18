use std::collections::HashMap;
use std::sync::Mutex;
use std::hash::{Hash, Hasher, BuildHasher};
use crate::table::Table as TableTrait;
use crate::element::{Element, ElementGuard};
use crate::alloc::{ABox, sarc_new, sarc_add_copy, sarc_remove_copy, sarc_deref};
use std::borrow::Borrow;

struct KeyWrapper<K, V>(pub *mut ABox<Element<K, V>>);

impl<K, V> KeyWrapper<K, V> {
    fn key(&self) -> &K {
        &sarc_deref(self.0).key
    }
}

impl<K: Eq, V> PartialEq for KeyWrapper<K, V> {
    fn eq(&self, other: &Self) -> bool {
        let d1 = sarc_deref(self.0);
        let d2 = sarc_deref(other.0);
        d1.key == d2.key
    }
}

impl<K: Eq, V> Eq for KeyWrapper<K, V> {}

impl<K: Hash, V> Hash for KeyWrapper<K, V> {
    fn hash<H>(&self, hasher: &mut H)
    where
        H: Hasher,
    {
        let d = sarc_deref(self.0);
        d.key.hash(hasher);
    }
}

struct ValueWrapper<K, V>(pub *mut ABox<Element<K, V>>);

impl<K, V> ValueWrapper<K, V> {
    fn value(&self) -> &V {
        &sarc_deref(self.0).value
    }

    fn guard(self) -> ElementGuard<K, V> {
        Element::read(self.0)
    }
}

pub struct Table<K, V, S> {
    inner: Mutex<HashMap<KeyWrapper<K, V>, ValueWrapper<K, V>, S>>,
}

impl<K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> TableTrait<K, V, S> for Table<K, V, S> {
    type Iter = Box<dyn Iterator<Item = ElementGuard<K, V>> + Send + Sync>;

    fn iter(&self) -> Self::Iter {
        todo!()
    }

    fn new(capacity: usize, build_hasher: S) -> Self {
        Self {
            inner: Mutex::new(HashMap::with_capacity_and_hasher(capacity, build_hasher)),
        }
    }

    fn insert_and_get(&self, key: K, value: V) -> ElementGuard<K, V> {
        todo!()
    }

    fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>> {
        let boxed = sarc_new(Element::new(key, 0, value));
        self.inner.lock().unwrap().insert(KeyWrapper(boxed), ValueWrapper(boxed)).map(ValueWrapper::guard)
    }

    fn get<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash
    {
        todo!()
    }

    fn remove_if_take<Q>(
        &self,
        search_key: &Q,
        predicate: &mut impl FnMut(&K, &V) -> bool,
    ) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash
    {
        todo!()
    }


    fn remove<Q>(&self, search_key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.remove_if(search_key, &mut |_, _| true)
    }

    fn insert(&self, key: K, value: V) -> bool {
        self.replace(key, value).is_none()
    }

    fn update_get<Q, F>(&self, search_key: &Q, do_update: &mut F) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V
    {
        todo!()
    }

    fn retain(&self, predicate: &mut impl FnMut(&K, &V) -> bool) {
        self.inner.lock().unwrap().retain(|k, v| predicate(k.key(), v.value()))
    }

    fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    fn capacity(&self) -> usize {
        self.inner.lock().unwrap().capacity()
    }
}
