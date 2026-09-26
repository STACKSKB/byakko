use serde::{Deserialize, Serialize};
pub mod archive;
pub mod keymap;
pub mod lighting;
pub mod macros;
pub mod picture;
pub mod settings;
/// Provenance for a device snapshot returned after reading or writing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum SnapshotEvidence {
    /// The value came from a device read.
    #[default]
    Readback,
    /// The device accepted a write, but a later read is needed to observe it.
    TransportAccepted,
}
