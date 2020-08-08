#![warn(missing_docs)]
#![deny(clippy::correctness)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]
#![warn(clippy::perf)]
#![warn(clippy::cargo)]

//! `dashmap` provides a ludicrously fast lockfree concurrent hash table.
//! There is no transaction support but a retryable compare-and-swap primitive is provided.
//! It is the core building block needed to implement a transactional layer on top, should it be needed.

pub mod alloc;
mod bucket;
mod entry_manager;
mod gc;
mod range;
mod shim;
mod thread_local;

pub use bucket::Guard;
