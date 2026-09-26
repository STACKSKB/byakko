//! Iced owns presentation and effect delivery; core owns the application model.
mod app;
pub mod config;
mod keymap;
pub mod panels;
pub mod physical_board;

pub use app::run;
