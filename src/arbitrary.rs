use arbitrary::{Arbitrary, Unstructured};
use core::hash::{BuildHasher, Hash};

impl<'a, K, V, S> Arbitrary<'a> for crate::DashMap<K, V, S>
where
    K: Eq + Hash + Arbitrary<'a>,
    V: Arbitrary<'a>,
    S: Default + BuildHasher + Clone,
{
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        u.arbitrary_iter()?.collect()
    }
}
