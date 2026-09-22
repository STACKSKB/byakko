//! Legacy root API for the Nia87 keymap adapter.
pub use byakko_devices::nia87::adapter::{
    BACKEND_ID, Nia87Adapter, action_from_raw, descriptor, draft_snapshot, from_snapshot, key_id,
    raw_from_action, to_snapshot,
};

use super::{Change, Descriptor, KeymapBackend, State};
use std::path::Path;

impl KeymapBackend for Nia87Adapter {
    fn descriptor(&self) -> Descriptor {
        Nia87Adapter::descriptor(self)
    }

    fn validate(&self, expected: &State, changes: &[Change]) -> Result<(), String> {
        Nia87Adapter::validate(self, expected, changes)
    }

    fn read(&self) -> Result<State, String> {
        Nia87Adapter::read(self)
    }

    fn apply(
        &self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, String> {
        Nia87Adapter::apply(self, expected, changes, backup_dir)
    }
}
