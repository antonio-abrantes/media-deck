//! Physical media profile I/O.

pub mod filesystem;
pub mod floppy;
#[cfg(windows)]
pub mod optical;
pub mod staging;
