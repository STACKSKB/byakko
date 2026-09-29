//! Nia87 board-facing facade. Feature protocols remain in their owned modules.
mod access;
mod apply_error;
mod configuration;
mod keymaps;
mod lighting;
mod macros;
mod picture;
mod settings;
mod transaction;
mod transport;

use crate::hid::HidDevice;
use apply_error::ApplyResult;
#[cfg(test)]
use apply_error::keymap_apply_error;

pub use access::Access;
use access::Selection;
pub use configuration::{apply_configuration, capture_configuration};
pub use keymaps::{apply_keymaps, snapshot};
pub use lighting::{HostLightingSession, apply_lighting, read_lighting};
pub use macros::{apply_macro, read_macro};
pub use picture::{apply_picture, read_picture};
pub use settings::{apply_setting, read_settings};
pub use transport::{
    Availability, Candidate, Target, TargetSelectionError, availability, candidates, descriptor,
    inspect, open_expected, open_unique,
};
use transport::{FeatureSetter, Session, read_payload, transaction_lock};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
use byakko_protocol::nia87::adapter::Snapshot;

use keymaps::{snapshot_on_device, write_binding};
#[cfg(test)]
use lighting::lighting_matches_report;
use lighting::{lighting_restore_report, read_lighting_on_device, write_lighting_report};
use macros::{read_macro_on_device, write_macro_bytes};
use picture::read_picture_on_device;
use settings::read_settings_on_device;

#[cfg(test)]
mod tests;
