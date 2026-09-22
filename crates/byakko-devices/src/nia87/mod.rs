//! Stock-firmware Nia87 backend. All packet and matrix details are scoped here.
pub mod adapter;
pub use adapter::*;

pub mod actions;
pub mod board;
pub mod configuration;
pub mod configuration_plan;
pub mod device;
pub mod host_lighting;
pub mod layout;
pub mod lighting;
pub mod lighting_adapter;
pub mod macro_adapter;
pub mod macro_file;
pub mod macros;
pub mod picture_adapter;
pub mod profiles;
pub mod protocol;
mod recovery_keymaps;
mod recovery_verification;
pub mod settings;
