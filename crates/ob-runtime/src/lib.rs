//! Sidecar supervisor: spawn, health, stop, port probing.

pub mod port;
pub mod supervisor;

pub use port::*;
pub use supervisor::*;
