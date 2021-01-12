use crate::mapref::entry::Entry;
use crate::DashMap;

use core::borrow::Borrow;
use core::fmt;
use core::hash::{BuildHasher, Hash};
use std::collections::hash_map::RandomState;

use stable_borrow::StableBorrow;

/// An intrusive view into a [`DashMap`]. Allows obtaining raw references to
/// the contents of stored values while maintaining the ability to add items to
/// the map.
pub struct ContentsView<K, V, S = RandomState> {
    map: DashMap<K, V, S>,
}

impl<K: Eq + Hash + Clone, V: Clone, S: Clone> Clone for ContentsView<K, V, S> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
        }
    }
}

impl<K: Eq + Hash + fmt::Debug, V: fmt::Debug, S: BuildHasher + Clone> fmt::Debug
    for ContentsView<K, V, S>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.map.fmt(f)
    }
}

impl<K, V, S> ContentsView<K, V, S> {
    pub(crate) fn new(map: DashMap<K, V, S>) -> Self {
        Self { map }
    }

    /// Consumes this `ContentsView`, returning the underlying `DashMap`.
    pub fn into_inner(self) -> DashMap<K, V, S> {
        self.map
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: BuildHasher + Clone> ContentsView<K, V, S> {
    /// Returns the number of elements in the map.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the map contains no elements.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the number of elements the map can hold without reallocating.
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }

    /// Returns `true` if the map contains a value for the specified key.
    pub fn contains_key<Q>(&'a self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(key)
    }

    /// Returns a reference to the contents of the value corresponding to the key.
    pub fn get<Q, B>(&'a self, key: &Q) -> Option<&'a B>
    where
        K: Borrow<Q>,
        V: StableBorrow<B>,
        Q: Hash + Eq + ?Sized,
        B: ?Sized,
    {
        // Safety: See get_key_value().
        self.map
            .get(key)
            .map(|guard| unsafe { borrow_stable(guard.value()) })
    }

    /// Returns the contents of the key-value pair corresponding to the supplied
    /// key.
    pub fn get_key_value<Q, C, B>(&'a self, key: &Q) -> Option<(&'a C, &'a B)>
    where
        K: Borrow<Q> + StableBorrow<C>,
        V: StableBorrow<B>,
        Q: Hash + Eq + ?Sized,
        C: ?Sized + 'a,
        B: ?Sized + 'a,
    {
        // Safety: T: StableBorrow<U> means that t.borrow() remains a valid reference as
        // long as t is not mutated. We never mutate or drop 't' (though we may move it,
        // which is allowed). When the DashMap is in ContentsView, methods to mutate or
        // drop existing values are not provided. Those values may move as a result of
        // re-allocation, but that's fine since the borrows from them are stable, as
        // required by StableBorrow.
        self.map
            .get(key)
            .map(|guard| unsafe { (borrow_stable(guard.key()), borrow_stable(guard.value())) })
    }

    /// An iterator visiting the contents of all key-value pairs in arbitrary order.
    /// The iterator element type is `(&'a C, &'a B)`.
    pub fn iter<C, B>(&'a self) -> impl Iterator<Item = (&'a C, &'a B)> + 'a
    where
        K: StableBorrow<C>,
        V: StableBorrow<B>,
        C: ?Sized + 'a,
        B: ?Sized + 'a,
    {
        // Safety: See get_key_value().
        self.map
            .iter()
            .map(|guard| unsafe { (borrow_stable(guard.key()), borrow_stable(guard.value())) })
    }

    /// An iterator visiting the contents all keys in arbitrary order. The iterator
    /// element type is `&'a B`.
    pub fn keys<B>(&'a self) -> impl Iterator<Item = &'a B> + 'a
    where
        K: StableBorrow<B>,
        B: ?Sized + 'a,
    {
        // Safety: See get_key_value().
        self.map
            .iter()
            .map(|guard| unsafe { borrow_stable(guard.key()) })
    }

    /// An iterator visiting all values in arbitrary order. The iterator element
    /// type is `&'a B`.
    pub fn values<B>(&'a self) -> impl Iterator<Item = &'a B> + 'a
    where
        V: StableBorrow<B>,
        B: ?Sized + 'a,
    {
        // Safety: See get_key_value().
        self.map
            .iter()
            .map(|guard| unsafe { borrow_stable(guard.value()) })
    }

    pub fn get_or_insert<B>(&'a self, key: K, value: V) -> &'a B
    where
        V: StableBorrow<B>,
        B: ?Sized + 'a,
    {
        // Safety: See get_key_value().
        match self.map.entry(key) {
            Entry::Occupied(entry) => unsafe { borrow_stable(entry.get()) },
            Entry::Vacant(entry) => unsafe { borrow_stable(entry.insert(value).value()) },
        }
    }

    pub fn get_or_insert_with<B, F>(&'a self, key: K, func: F) -> &'a B
    where
        V: StableBorrow<B>,
        B: ?Sized + 'a,
        F: FnOnce(&K) -> V,
    {
        // Safety: See get_key_value().
        match self.map.entry(key) {
            Entry::Occupied(entry) => unsafe { borrow_stable(entry.get()) },
            Entry::Vacant(entry) => unsafe {
                let value = func(entry.key());
                borrow_stable(entry.insert(value).value())
            },
        }
    }
}

// Safety: 'value' must not be mutated for 'a
unsafe fn borrow_stable<'a, 'b, T, B>(value: &'b T) -> &'a B
where
    T: StableBorrow<B> + ?Sized + 'a,
    B: ?Sized + 'a,
{
    &*(value.borrow() as *const B)
}

#[cfg(test)]
mod tests {
    use crate::DashMap;
    use std::borrow::Borrow;

    fn construct_sample_map() -> DashMap<String, String> {
        let map = DashMap::new();

        map.insert("a".to_string(), "one".to_string());
        map.insert("b".to_string(), "two".to_string());
        map.insert("c".to_string(), "three".to_string());
        map.insert("d".to_string(), "four".to_string());

        map
    }

    #[test]
    fn test_properties() {
        let map = construct_sample_map();

        let view = map.clone().into_contents_view();
        assert_eq!(view.is_empty(), map.is_empty());
        assert_eq!(view.len(), map.len());
        assert_eq!(view.capacity(), map.capacity());

        let new_map = view.into_inner();
        assert_eq!(new_map.is_empty(), map.is_empty());
        assert_eq!(new_map.len(), map.len());
        assert_eq!(new_map.capacity(), map.capacity());
    }

    #[test]
    fn test_get() {
        let map = construct_sample_map();
        let view = map.clone().into_contents_view();

        for guard in map.iter() {
            let key = guard.key();
            let value = guard.value();

            assert!(view.contains_key(key));
            assert_eq!(Some(value.borrow()), view.get(key));
            assert_eq!(
                Some((key.borrow(), value.borrow())),
                view.get_key_value(key)
            );
        }

        assert_eq!(
            view.get_or_insert("a".to_string(), "dupe".to_string()),
            "one"
        );
        assert_eq!(
            view.get_or_insert("e".to_string(), "five".to_string()),
            "five"
        );

        for guard in map.iter() {
            let key = guard.key();
            let value = guard.value();

            assert!(view.contains_key(key));
            assert_eq!(Some(value.borrow()), view.get(key));
            assert_eq!(
                Some((key.borrow(), value.borrow())),
                view.get_key_value(key)
            );
        }
    }
}
