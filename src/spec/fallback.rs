use crate::alloc::{sarc_deref, sarc_new, sarc_remove_copy, ABox};
use crate::element::{Element, ElementGuard};
use crate::table::Table as TableTrait;
use std::borrow::Borrow;
use std::cmp::max;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

type T<K, V> = *mut ABox<Element<K, V>>;

pub struct Table<K, V, S> {
    inner: RwLock<Arc<Box<[RwLock<Option<T<K, V>>>]>>>,
    len: AtomicUsize,
    hasher: S,
}

impl<K, V, S: BuildHasher> Table<K, V, S> {
    fn hash<Q>(&self, v: &Q) -> u64
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let mut hasher = self.hasher.build_hasher();
        v.hash(&mut hasher);
        hasher.finish()
    }
}

struct Iter<K, V> {
    table: Arc<Box<[RwLock<Option<T<K, V>>>]>>,
    position: usize,
}

impl<K: Eq + Hash, V> Iterator for Iter<K, V> {
    type Item = ElementGuard<K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position == self.table.len() {
            None
        } else {
            let r = self.table[self.position]
                .read()
                .unwrap()
                .map(|g| Element::read(g));

            self.position += 1;

            if r.is_none() {
                self.next()
            } else {
                r
            }
        }
    }
}

impl<K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> Table<K, V, S> {
    fn grow_check(&self) {
        let cap = self.capacity();
        let len = self.len.load(Ordering::SeqCst);
        let threshold = (cap as f32 * 0.75) as usize;
        let new_cap = cap * 2;

        if len > threshold {
            let mut buckets = Vec::with_capacity(new_cap);

            for _ in 0..new_cap {
                buckets.push(RwLock::new(None));
            }

            let iter = self.iter();
            let mut inner = self.inner.write().unwrap();
            *inner = Arc::new(buckets.into_boxed_slice());

            for guard in iter {
                self.insert_elem(guard);
            }
        }
    }

    fn insert_elem(&self, guard: ElementGuard<K, V>) {
        let inner = self.inner.read().unwrap();
        let mut idx = self.hash(guard.key()) as usize % inner.len();
        let boxed = guard.mem_guard;

        loop {
            let mut slot = inner[idx].write().unwrap();

            match *slot {
                Some(e) => {
                    let data = sarc_deref(e);
                    let data_new = sarc_deref(boxed);

                    if data.key == data_new.key {
                        sarc_remove_copy(e);
                        *slot = Some(boxed);
                        self.len.fetch_sub(1, Ordering::SeqCst);
                        break;
                    } else {
                        idx += 1;
                    }
                }

                None => {
                    *slot = Some(boxed);
                    break;
                }
            }
        }

        self.len.fetch_add(1, Ordering::SeqCst);
    }
}

impl<K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> TableTrait<K, V, S>
    for Table<K, V, S>
{
    type Iter = Box<dyn Iterator<Item = ElementGuard<K, V>>>;

    fn iter(&self) -> Self::Iter {
        let table = self.inner.read().unwrap().clone();

        Box::new(Iter { table, position: 0 })
    }

    fn new(mut capacity: usize, hasher: S) -> Self {
        capacity = max(capacity, 8);

        let mut buckets = Vec::with_capacity(capacity);

        for _ in 0..capacity {
            buckets.push(RwLock::new(None));
        }

        Self {
            inner: RwLock::new(Arc::new(buckets.into_boxed_slice())),
            len: AtomicUsize::new(0),
            hasher,
        }
    }

    fn insert_and_get(&self, key: K, value: V) -> ElementGuard<K, V> {
        let inner = self.inner.read().unwrap();
        let mut idx = self.hash(&key) as usize % inner.len();
        let boxed = sarc_new(Element::new(key, 0, value));

        loop {
            let mut slot = inner[idx].write().unwrap();

            match *slot {
                Some(e) => {
                    let data = sarc_deref(e);
                    let data_new = sarc_deref(boxed);

                    if data.key == data_new.key {
                        sarc_remove_copy(e);
                        *slot = Some(boxed);
                        self.len.fetch_sub(1, Ordering::SeqCst);
                        break;
                    } else {
                        idx += 1;
                    }
                }

                None => {
                    *slot = Some(boxed);
                    break;
                }
            }
        }

        self.len.fetch_add(1, Ordering::SeqCst);
        drop(inner);
        self.grow_check();
        Element::read(boxed)
    }

    fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>> {
        let inner = self.inner.read().unwrap();
        let mut idx = self.hash(&key) as usize % inner.len();
        let boxed = sarc_new(Element::new(key, 0, value));

        loop {
            let mut slot = inner[idx].write().unwrap();

            match *slot {
                Some(e) => {
                    let data = sarc_deref(e);
                    let data_new = sarc_deref(boxed);

                    if data.key == data_new.key {
                        let ew = Element::read(e);
                        sarc_remove_copy(e);
                        *slot = Some(boxed);
                        drop(slot);
                        drop(inner);
                        self.grow_check();
                        break Some(ew);
                    } else {
                        idx += 1;
                    }
                }

                None => {
                    *slot = Some(boxed);
                    self.len.fetch_add(1, Ordering::SeqCst);
                    drop(slot);
                    drop(inner);
                    self.grow_check();
                    break None;
                }
            }
        }
    }

    fn get<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let inner = self.inner.read().unwrap();
        let mut idx = self.hash(search_key) as usize % inner.len();

        loop {
            let slot = inner[idx].read().unwrap();

            match *slot {
                Some(e) => {
                    if sarc_deref(e).key.borrow() == search_key {
                        break Some(Element::read(e));
                    } else {
                        idx += 1;
                    }
                }

                None => break None,
            }
        }
    }

    fn remove_if_take<Q>(
        &self,
        search_key: &Q,
        predicate: &mut impl FnMut(&K, &V) -> bool,
    ) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        let inner = self.inner.read().unwrap();
        let mut idx = self.hash(search_key) as usize % inner.len();

        loop {
            let mut slot = inner[idx].write().unwrap();

            match *slot {
                Some(e) => {
                    let data = sarc_deref(e);
                    if data.key.borrow() == search_key && predicate(&data.key, &data.value) {
                        let _guard = Element::read(e);
                        sarc_remove_copy(e);
                        *slot = None;
                        self.len.fetch_sub(1, Ordering::SeqCst);
                    } else {
                        idx += 1;
                    }
                }

                None => break None,
            }
        }
    }

    fn update_get<Q, F>(&self, search_key: &Q, do_update: &mut F) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        let inner = self.inner.read().unwrap();
        let mut idx = self.hash(search_key) as usize % inner.len();

        loop {
            let mut slot = inner[idx].write().unwrap();

            match *slot {
                Some(e) => {
                    let data = sarc_deref(e);
                    if data.key.borrow() == search_key {
                        let new_key = data.key.clone();
                        let new_value = do_update(&data.key, &data.value);
                        sarc_remove_copy(e);
                        let boxed = sarc_new(Element::new(new_key, 0, new_value));
                        let guard = Element::read(boxed);
                        *slot = Some(boxed);
                        return Some(guard);
                    } else {
                        idx += 1;
                    }
                }

                None => break None,
            }
        }
    }

    fn retain(&self, predicate: &mut impl FnMut(&K, &V) -> bool) {
        let inner = self.inner.read().unwrap();

        for slot in &***inner {
            let mut slot = slot.write().unwrap();

            if let Some(e) = *slot {
                let data = sarc_deref(e);

                if !predicate(&data.key, &data.value) {
                    sarc_remove_copy(e);
                    *slot = None;
                    self.len.fetch_sub(1, Ordering::SeqCst);
                }
            }
        }
    }

    fn len(&self) -> usize {
        self.len.load(Ordering::SeqCst)
    }

    fn capacity(&self) -> usize {
        self.inner.read().unwrap().len()
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for Table<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for Table<K, V, S> {}
