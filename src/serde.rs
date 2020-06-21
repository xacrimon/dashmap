use crate::DashMap;
use crate::DashSet;
use core::fmt;
use core::hash::Hash;
use serde::de::{Deserialize, MapAccess, SeqAccess, Visitor};
use serde::export::PhantomData;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde::Deserializer;
use std::ops::Deref;

pub struct DashMapVisitor<K, V> {
    marker: PhantomData<fn() -> DashMap<K, V>>,
}

impl<K: 'static, V: 'static> DashMapVisitor<K, V>
where
    K: Eq + Hash,
{
    fn new() -> Self {
        DashMapVisitor {
            marker: PhantomData,
        }
    }
}

impl<'de, K: 'static, V: 'static> Visitor<'de> for DashMapVisitor<K, V>
where
    K: Deserialize<'de> + Eq + Hash,
    V: Deserialize<'de>,
{
    type Value = DashMap<K, V>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a DashMap")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let map = DashMap::with_capacity(access.size_hint().unwrap_or(0));

        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }

        Ok(map)
    }
}

impl<'de, K: 'static, V: 'static> Deserialize<'de> for DashMap<K, V>
where
    K: Deserialize<'de> + Eq + Hash,
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(DashMapVisitor::<K, V>::new())
    }
}

impl<K: 'static, V: 'static> Serialize for DashMap<K, V>
where
    K: Serialize + Eq + Hash,
    V: Serialize,
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

pub struct DashSetVisitor<K> {
    marker: PhantomData<fn() -> DashSet<K>>,
}

impl<K: 'static> DashSetVisitor<K>
where
    K: Eq + Hash,
{
    fn new() -> Self {
        DashSetVisitor {
            marker: PhantomData,
        }
    }
}

impl<'de, K: 'static> Visitor<'de> for DashSetVisitor<K>
where
    K: Deserialize<'de> + Eq + Hash,
{
    type Value = DashSet<K>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a DashMap")
    }

    fn visit_seq<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: SeqAccess<'de>,
    {
        let set = DashSet::with_capacity(access.size_hint().unwrap_or(0));

        while let Some(key) = access.next_element()? {
            set.insert(key);
        }

        Ok(set)
    }
}

impl<'de, K: 'static> Deserialize<'de> for DashSet<K>
where
    K: Deserialize<'de> + Eq + Hash,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(DashSetVisitor::<K>::new())
    }
}

impl<K: 'static> Serialize for DashSet<K>
where
    K: Serialize + Eq + Hash,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut set = serializer.serialize_seq(Some(self.len()))?;
        for ref_multi in self.iter() {
            set.serialize_element(ref_multi.deref())?;
        }
        set.end()
    }
}
