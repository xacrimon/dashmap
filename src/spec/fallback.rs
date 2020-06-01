use crate::alloc::{sarc_deref, sarc_new, ABox};
use crate::element::{Element, ElementGuard};
use crate::table::Table as TableTrait;
use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::sync::Mutex;

type T<K, V> = *mut ABox<Element<K, V>>;

/// This is a bit of a temporary fallback.
/// But it works fow now.
/// If you aren't on amd64 or POWER9 chances are concurrent
/// performance isn't super high on your list.
pub struct Table<K, V, S> {
    inner: Mutex<Vec<T<K, V>>>,
    _phantom: PhantomData<S>,
}

impl<K: Eq + Hash + 'static, V: 'static, S: BuildHasher + 'static> TableTrait<K, V, S>
    for Table<K, V, S>
{
    type Iter = Box<dyn Iterator<Item = ElementGuard<K, V>>>;

    fn iter(&self) -> Self::Iter {
        Box::new(
            self.inner
                .lock()
                .unwrap()
                .clone()
                .into_iter()
                .map(|sptr| Element::read(sptr)),
        )
    }

    fn new(capacity: usize, _build_hasher: S) -> Self {
        Self {
            inner: Mutex::new(Vec::with_capacity(capacity)),
            _phantom: PhantomData,
        }
    }

    fn insert_and_get(&self, key: K, value: V) -> ElementGuard<K, V> {
        let boxed = sarc_new(Element::new(key, 0, value));
        let mut inner = self.inner.lock().unwrap();
        inner.retain(|t| *t != boxed);
        inner.push(boxed);
        Element::read(boxed)
    }

    fn replace(&self, key: K, value: V) -> Option<ElementGuard<K, V>> {
        let boxed = sarc_new(Element::new(key, 0, value));
        let mut inner = self.inner.lock().unwrap();
        let mut r = None;

        inner.retain(|sptr| {
            let sd = sarc_deref(*sptr);
            let bd = sarc_deref(boxed);
            if sd.key == bd.key {
                r = Some(Element::read(*sptr));
                false
            } else {
                true
            }
        });

        inner.push(boxed);
        r
    }

    fn get<Q>(&self, search_key: &Q) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .find(|sptr| sarc_deref(**sptr).key.borrow() == search_key)
            .map(|sptr| Element::read(*sptr))
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
        let mut inner = self.inner.lock().unwrap();
        let iter = inner.iter().enumerate();
        let mut tr = None;

        for (i, sptr) in iter {
            let sd = sarc_deref(*sptr);
            if sd.key.borrow() == search_key && predicate(&sd.key, &sd.value) {
                tr = Some(i);
                break;
            }
        }

        tr.map(|i| {
            let guard = Element::read(inner[i]);
            inner.remove(i);
            guard
        })
    }

    fn update_get<Q, F>(&self, search_key: &Q, do_update: &mut F) -> Option<ElementGuard<K, V>>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Eq + Hash,
        F: FnMut(&K, &V) -> V,
    {
        let mut inner = self.inner.lock().unwrap();
        let iter = inner.iter().enumerate();
        let mut apply = None;

        for (i, sptr) in iter {
            let sd = sarc_deref(*sptr);
            if sd.key.borrow() == search_key {
                let new_value = do_update(&sd.key, &sd.value);
                let key = sd.key.clone();
                let boxed = sarc_new(Element::new(key, 0, new_value));
                apply = Some((i, boxed));
                break;
            }
        }

        if let Some((i, new)) = apply {
            inner[i] = new;
        }

        None
    }

    fn retain(&self, predicate: &mut impl FnMut(&K, &V) -> bool) {
        self.inner.lock().unwrap().retain(|sptr| {
            let sd = sarc_deref(*sptr);
            predicate(&sd.key, &sd.value)
        })
    }

    fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    fn capacity(&self) -> usize {
        self.inner.lock().unwrap().capacity()
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for Table<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for Table<K, V, S> {}
