//! Runtime keymap backend contract; shared values live in byakko-core.
pub use byakko_core::{
    Action, ActionChoice, Change, Descriptor, Layer, PhysicalKey, State, validate_changes,
    validate_state,
};
use std::path::Path;
pub mod nia87;

pub trait KeymapBackend: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    fn read(&self) -> Result<State, String>;
    fn validate(&self, expected: &State, changes: &[Change]) -> Result<(), String> {
        let descriptor = self.descriptor();
        validate_state(&descriptor, expected)?;
        validate_changes(&descriptor, changes)
    }
    fn apply(
        &self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, String>;
}
