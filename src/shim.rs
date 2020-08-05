#[cfg(not(test))]
pub use std::{alloc, sync, thread};

#[cfg(test)]
pub use loom::{alloc, sync, thread};
