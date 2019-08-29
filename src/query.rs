use crate::mapref::one::{DashMapRef, DashMapRefMut};
use crate::DashMap;
use owning_ref::{OwningRef, OwningRefMut};
use std::borrow::Borrow;
use std::hash::Hash;
use std::marker::PhantomData;

pub trait ExecutableQuery {
    type Output;
    fn exec(self) -> Self::Output;
}

pub trait DataProvider {
    type Data;
    fn get_data(&mut self) -> Self::Data;
}

pub trait MapProvider<'a, K: Eq + Hash, V> {
    fn get_map(&'a self) -> &'a DashMap<K, V>;
}

// -- Query

pub struct Query<'a, K: Eq + Hash, V> {
    map: &'a DashMap<K, V>,
}

impl<'a, K: Eq + Hash, V> Query<'a, K, V> {
    pub fn new(map: &'a DashMap<K, V>) -> Self {
        map.transaction_lock().acquire_shared();
        Self { map }
    }
}

impl<'a, K: Eq + Hash, V> MapProvider<'a, K, V> for Query<'a, K, V> {
    fn get_map(&'a self) -> &'a DashMap<K, V> {
        &self.map
    }
}

impl<'a, K: Eq + Hash, V> Drop for Query<'a, K, V> {
    fn drop(&mut self) {
        unsafe {
            self.map.transaction_lock().release_shared();
        }
    }
}

// --

// -- QueryDataProvider

pub struct QueryDataProvider<'a, K: Eq + Hash, V, Q: MapProvider<'a, K, V>, T> {
    inner: Q,
    data: Option<T>,
    _phantom: PhantomData<&'a (K, V)>,
}

impl<'a, K: Eq + Hash, V, Q: MapProvider<'a, K, V>, T> QueryDataProvider<'a, K, V, Q, T> {
    pub fn new(inner: Q, data: T) -> Self {
        Self {
            inner,
            data: Some(data),
            _phantom: PhantomData,
        }
    }
}

impl<'a, K: Eq + Hash, V, Q: MapProvider<'a, K, V>, T> MapProvider<'a, K, V> for QueryDataProvider<'a, K, V, Q, T> {
    fn get_map(&'a self) -> &'a DashMap<K, V> {
        self.inner.get_map()
    }
}

impl<'a, K: Eq + Hash, V, Q: MapProvider<'a, K, V>, T> DataProvider for QueryDataProvider<'a, K, V, Q, T> {
    type Data = T;

    fn get_data(&mut self) -> Self::Data {
        self.data.take().unwrap()
    }
}

// --
