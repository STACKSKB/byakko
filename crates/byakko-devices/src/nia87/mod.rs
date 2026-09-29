//! Stock-firmware Nia87 backend. All packet and matrix details are scoped here.
pub mod adapter;
pub use adapter::*;

pub mod application;
pub mod archive_adapter;
pub mod configuration;
pub mod configuration_plan;
pub mod device;
mod host_adapter;
pub mod host_lighting;
pub mod lighting_adapter;
pub mod macro_adapter;
pub mod macro_file;
pub mod notifications;
pub mod picture_adapter;
pub mod profiles;
mod recovery_verification;
pub mod settings;
pub mod settings_adapter;
