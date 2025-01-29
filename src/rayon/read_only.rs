use crate::HashMap;
use crate::ReadOnlyView;
use core::hash::{BuildHasher, Hash};
use rayon::iter::plumbing::UnindexedConsumer;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

impl<K, V, S> IntoParallelIterator for ReadOnlyView<K, V, S>
where
    K: Send + Eq + Hash,
    V: Send,
    S: Send + BuildHasher,
{
    type Iter = ReadOnlyOwningIter<K, V>;
    type Item = (K, V);

    fn into_par_iter(self) -> Self::Iter {
        ReadOnlyOwningIter {
            shards: self.shards,
        }
    }
}

pub struct ReadOnlyOwningIter<K, V> {
    pub(super) shards: Box<[HashMap<K, V>]>,
}

impl<K, V> ParallelIterator for ReadOnlyOwningIter<K, V>
where
    K: Send + Eq + Hash,
    V: Send,
{
    type Item = (K, V);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        Vec::from(self.shards)
            .into_par_iter()
            .flat_map_iter(|shard| shard.into_iter())
            .drive_unindexed(consumer)
    }
}

// This impl also enables `IntoParallelRefIterator::par_iter`
impl<'a, K, V, S> IntoParallelIterator for &'a ReadOnlyView<K, V, S>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
    S: Send + Sync + BuildHasher,
{
    type Iter = ReadOnlyIter<'a, K, V>;
    type Item = &'a (K, V);

    fn into_par_iter(self) -> Self::Iter {
        ReadOnlyIter {
            shards: &self.shards,
        }
    }
}

pub struct ReadOnlyIter<'a, K, V> {
    pub(super) shards: &'a [HashMap<K, V>],
}

impl<'a, K, V> ParallelIterator for ReadOnlyIter<'a, K, V>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
{
    type Item = &'a (K, V);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        self.shards
            .into_par_iter()
            .flat_map_iter(|shard| shard.iter())
            .drive_unindexed(consumer)
    }
}

#[cfg(test)]
mod tests {
    use crate::ClashMap;
    use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};

    fn construct_sample_map() -> ClashMap<i32, String> {
        let map = ClashMap::new();

        map.insert(1, "one".to_string());

        map.insert(10, "ten".to_string());

        map.insert(27, "twenty seven".to_string());

        map.insert(45, "forty five".to_string());

        map
    }

    #[test]
    fn test_par_iter() {
        let map = construct_sample_map();

        let view = map.clone().into_read_only();

        view.par_iter().for_each(|entry| {
            let (key, _) = *entry;

            assert!(view.contains_key(&key));

            let map_entry = map.get(&key).unwrap();

            assert_eq!(view.get(&key).unwrap(), map_entry.value());

            let key_value: (&i32, &String) = view.get_key_value(&key).unwrap();

            assert_eq!(key_value.0, map_entry.key());

            assert_eq!(key_value.1, map_entry.value());
        });
    }

    #[test]
    fn test_into_par_iter() {
        let map = construct_sample_map();

        let view = map.clone().into_read_only();

        view.into_par_iter().for_each(|(key, value)| {
            let map_entry = map.get(&key).unwrap();

            assert_eq!(&key, map_entry.key());

            assert_eq!(&value, map_entry.value());
        });
    }
}
