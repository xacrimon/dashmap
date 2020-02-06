use std::hash::{BuildHasher, Hash, Hasher};
use std::marker::PhantomData;

#[derive(Clone, Default)]
pub struct ShardBuildHasher<S>(PhantomData<S>);

#[derive(Clone, Default, Debug)]
pub struct ShardHasher {
    hash: u64,
}

#[derive(Eq, Clone)]
pub struct ShardKey<K> {
    key: Option<K>,
    hash: u64,
}

impl<S> BuildHasher for ShardBuildHasher<S> {
    type Hasher = ShardHasher;

    fn build_hasher(&self) -> Self::Hasher {
        ShardHasher { hash: 0 }
    }
}

impl<S> ShardBuildHasher<S> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl Hasher for ShardHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, _: &[u8]) {}

    fn write_u64(&mut self, i: u64) {
        self.hash = i;
    }
}

impl<K> Hash for ShardKey<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash)
    }
}

impl<K> PartialEq for ShardKey<K> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl<K> ShardKey<K> {
    pub fn new(key: K, hash: u64) -> Self {
        Self {
            key: Some(key),
            hash,
        }
    }

    pub fn new_hash(hash: u64) -> Self {
        Self { key: None, hash }
    }

    pub fn into_inner(self) -> K {
        self.key.unwrap()
    }

    pub fn get(&self) -> &K {
        self.key.as_ref().unwrap()
    }
}
