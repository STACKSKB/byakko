//! Iced owns presentation and effect delivery; core owns the application model.
mod app;
pub mod config;
mod form;
mod view;
pub mod widget;

pub use app::run;
