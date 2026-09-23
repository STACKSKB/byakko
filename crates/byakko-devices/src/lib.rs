//! Native effect boundary. One worker owns the backend; the UI owns no HID I/O.
mod device;
mod executor;

pub use device::{Device, KeymapDevice};
pub use executor::Executor;

pub mod hid;
pub mod macro_files;
pub mod memory;
pub mod nia87;
#[cfg(feature = "research-tools")]
pub mod research_fault;
#[cfg(feature = "research-tools")]
pub mod research_trace;
pub mod rongyuan;
pub mod storage;
