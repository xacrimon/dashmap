use crate::{mapref, setref, ClashMap, ClashSet};
use core::fmt;
use core::hash::{BuildHasher, Hash};
use core::marker::PhantomData;
use serde::de::{Deserialize, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde::Deserializer;

type Contravariant<T> = PhantomData<fn() -> T>;
pub struct ClashMapVisitor<K, V, S> {
    marker: Contravariant<ClashMap<K, V, S>>,
}

impl<K, V, S> ClashMapVisitor<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn new() -> Self {
        ClashMapVisitor {
            marker: PhantomData,
        }
    }
}

impl<'de, K, V, S> Visitor<'de> for ClashMapVisitor<K, V, S>
where
    K: Deserialize<'de> + Eq + Hash,
    V: Deserialize<'de>,
    S: BuildHasher + Default,
{
    type Value = ClashMap<K, V, S>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a ClashMap")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let map =
            ClashMap::with_capacity_and_hasher(access.size_hint().unwrap_or(0), Default::default());

        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }

        Ok(map)
    }
}

impl<'de, K, V, S> Deserialize<'de> for ClashMap<K, V, S>
where
    K: Deserialize<'de> + Eq + Hash,
    V: Deserialize<'de>,
    S: BuildHasher + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(ClashMapVisitor::<K, V, S>::new())
    }
}

impl<K, V, H> Serialize for ClashMap<K, V, H>
where
    K: Serialize + Eq + Hash,
    V: Serialize,
    H: BuildHasher,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;

        for ref_multi in self.iter() {
            map.serialize_entry(ref_multi.key(), ref_multi.value())?;
        }

        map.end()
    }
}

pub struct ClashSetVisitor<K, S> {
    marker: PhantomData<fn() -> ClashSet<K, S>>,
}

impl<K, S> ClashSetVisitor<K, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn new() -> Self {
        ClashSetVisitor {
            marker: PhantomData,
        }
    }
}

impl<'de, K, S> Visitor<'de> for ClashSetVisitor<K, S>
where
    K: Deserialize<'de> + Eq + Hash,
    S: BuildHasher + Default,
{
    type Value = ClashSet<K, S>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a ClashSet")
    }

    fn visit_seq<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: SeqAccess<'de>,
    {
        let map =
            ClashSet::with_capacity_and_hasher(access.size_hint().unwrap_or(0), Default::default());

        while let Some(key) = access.next_element()? {
            map.insert(key);
        }

        Ok(map)
    }
}

impl<'de, K, S> Deserialize<'de> for ClashSet<K, S>
where
    K: Deserialize<'de> + Eq + Hash,
    S: BuildHasher + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(ClashSetVisitor::<K, S>::new())
    }
}

impl<K, H> Serialize for ClashSet<K, H>
where
    K: Serialize + Eq + Hash,
    H: BuildHasher,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;

        for ref_multi in self.iter() {
            seq.serialize_element(ref_multi.key())?;
        }

        seq.end()
    }
}

macro_rules! serialize_impl {
    () => {
        fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
        where
            Ser: serde::Serializer,
        {
            std::ops::Deref::deref(self).serialize(serializer)
        }
    };
}

// Map
impl<K: Eq + Hash, V: Serialize> Serialize for mapref::multiple::RefMulti<'_, K, V> {
    serialize_impl! {}
}

impl<K: Eq + Hash, V: Serialize> Serialize for mapref::multiple::RefMutMulti<'_, K, V> {
    serialize_impl! {}
}

impl<K: Eq + Hash, V: Serialize> Serialize for mapref::one::Ref<'_, K, V> {
    serialize_impl! {}
}

impl<K: Eq + Hash, V: Serialize> Serialize for mapref::one::RefMut<'_, K, V> {
    serialize_impl! {}
}

impl<K: Eq + Hash, T: Serialize> Serialize for mapref::one::MappedRef<'_, K, T> {
    serialize_impl! {}
}

impl<K: Eq + Hash, T: Serialize> Serialize for mapref::one::MappedRefMut<'_, K, T> {
    serialize_impl! {}
}

// Set
impl<V: Hash + Eq + Serialize> Serialize for setref::multiple::RefMulti<'_, V> {
    serialize_impl! {}
}

impl<V: Hash + Eq + Serialize> Serialize for setref::one::Ref<'_, V> {
    serialize_impl! {}
}
