#[cfg(not(feature = "loom"))]
pub use std::{sync, thread};

#[cfg(feature = "loom")]
pub use loom::{sync, thread};
