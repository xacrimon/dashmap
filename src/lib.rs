#![allow(unused_unsafe)]
#![warn(missing_docs)]
#![deny(clippy::correctness)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]
#![warn(clippy::perf)]
#![warn(clippy::cargo)]

//! `dashmap` provides a relatively low level high performance concurrent hash map.
//! See struct level docs for more details.
//!
//! Serde is supported if the `serde` feature is enabled.
//! Then `DashMap` will implement `Serialize` and `Deserialize`.

mod alloc;
mod element;
mod table;
