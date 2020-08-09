//! This is a small shim module that allows us to use loom types when model testing.

#[cfg(not(feature = "loom"))]
pub use std::{sync, thread};

#[cfg(feature = "loom")]
pub use loom::{sync, thread};
