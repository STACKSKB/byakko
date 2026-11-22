//! Iced owns presentation and effect delivery; core owns the application model.
mod app;
pub mod config;
mod controller;
mod form;
mod input;
#[cfg(target_os = "linux")]
mod linux_permissions;
mod view;
pub mod widget;

pub use app::run;
pub use controller::discovery::{Availability, Discovery, Endpoint};
