#![warn(missing_docs)]
#![deny(clippy::correctness)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]
#![warn(clippy::perf)]
#![warn(clippy::cargo)]

//! `dashmap` provides a ludicrously fast lock-free concurrent hash table.
//! There is no transaction support but a retryable compare-and-swap primitive is provided.
//! It is the core building block needed to implement a transactional layer on top, should it be needed.

pub mod alloc;
mod bucket;
mod entry_manager;
mod gc;
mod thread_local;
mod utils;

pub use bucket::Guard;
