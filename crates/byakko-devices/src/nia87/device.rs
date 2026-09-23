//! Nia87 board-facing facade. Feature protocols remain in their owned modules.
mod access;
mod apply_error;
mod configuration;
mod keymaps;
mod lighting;
mod macros;
mod picture;
mod settings;
mod transport;

use crate::hid::HidDevice;
use apply_error::detailed;
#[cfg(test)]
use apply_error::keymap_apply_error;
use serde::{Deserialize, Serialize};

pub use access::Access;
use access::Selection;
pub use configuration::{apply_configuration, apply_configuration_detailed, capture_configuration};
pub use keymaps::{
    apply_keymaps, apply_keymaps_detailed, apply_keymaps_detailed_for, apply_keymaps_for, snapshot,
    snapshot_for,
};
pub use lighting::{
    HostLightingSession, ScreenSession, apply_lighting, apply_lighting_detailed,
    apply_lighting_detailed_for, apply_lighting_for, read_lighting, read_lighting_for,
};
pub use macros::{
    apply_macro, apply_macro_detailed, apply_macro_detailed_for, apply_macro_for, read_macro,
    read_macro_for,
};
pub use picture::{
    apply_picture, apply_picture_detailed, apply_picture_detailed_for, apply_picture_for,
    read_picture, read_picture_for,
};
pub use settings::{
    apply_setting, apply_setting_detailed, apply_setting_detailed_for, apply_setting_for,
    read_settings, read_settings_for,
};
pub use transport::{
    Availability, Candidate, Target, TargetSelectionError, availability, candidates, descriptor,
    inspect, open_expected, open_unique,
};
use transport::{FeatureSetter, Session, read_payload, transaction_lock};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub format_version: u32,
    pub firmware: u16,
    pub profile: u8,
    pub base: Vec<[u8; 4]>,
    pub function: Vec<[u8; 4]>,
}

use keymaps::{snapshot_on_device, snapshot_unlocked, write_binding};
#[cfg(test)]
use lighting::lighting_matches_report;
use lighting::{lighting_restore_report, read_lighting_on_device, write_lighting_report};
#[cfg(test)]
use macros::stable_macro_reads;
use macros::{read_macro_on_device, write_macro_bytes};
use picture::read_picture_on_device;
#[cfg(test)]
use picture::stable_picture_reads;
use settings::read_settings_on_device;

#[cfg(test)]
mod tests;
