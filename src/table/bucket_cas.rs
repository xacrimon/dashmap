use flize::{ebr::Ebr, function_runner::{FunctionRunner, Function}, Atomic, Shared, Shield};
use std::hash::Hash;
use crate::{alloc::ObjectAllocator, bucket::Bucket};
use super::tag::BTag;

trait BucketCas {
    type K: Eq + Hash;
    type V;
    type A: ObjectAllocator<Bucket<Self::K, Self::V, Self::A>>;
}
