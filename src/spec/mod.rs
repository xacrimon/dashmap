//#[cfg(target_arch = "x86_64")]
//pub mod x86_64;
//pub use x86_64::Table;

//#[cfg(not(target_arch = "x86_64"))]
pub mod fallback;
pub use fallback::Table;
