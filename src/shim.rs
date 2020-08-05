#[cfg(not(test))]
pub use std::{sync, thread};

#[cfg(test)]
pub use loom::{sync, thread};
