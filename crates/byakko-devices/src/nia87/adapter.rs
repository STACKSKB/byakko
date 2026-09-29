//! Native Nia87 adapter and ordered device effects.
use crate::nia87::{device, macro_adapter};
use byakko_core::model::{
    keymap::{Change, State},
    macros,
};
use byakko_protocol::nia87::adapter::{draft_snapshot, from_snapshot, revision_snapshot};
use std::{collections::BTreeMap, path::Path};

pub struct BoundNia87Adapter {
    access: device::Access,
}
impl BoundNia87Adapter {
    pub fn new(target: device::Target) -> Self {
        Self {
            access: device::Access::bound(target),
        }
    }
}

fn apply_keymap(
    access: &device::Access,
    expected: &State,
    changes: &[Change],
    backup_dir: &Path,
) -> Result<State, byakko_core::contract::ApplyFailure> {
    use byakko_core::contract::{ApplyFailure, Recovery};
    let prepare = || -> Result<_, String> {
        Ok((
            revision_snapshot(expected)?,
            draft_snapshot(expected, changes)?,
        ))
    };
    let (original, draft) = prepare().map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let actual = access.apply_keymaps(&original, &draft.base, &draft.function, backup_dir)?;
    from_snapshot(&actual).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::Unverified,
    })
}

impl crate::Device for BoundNia87Adapter {
    fn read(&mut self) -> Result<State, String> {
        self.access
            .snapshot()
            .map_err(|error| error.to_string())
            .and_then(|snapshot| from_snapshot(&snapshot))
    }

    fn apply(
        &mut self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, byakko_core::contract::ApplyFailure> {
        apply_keymap(&self.access, expected, changes, backup_dir)
    }

    fn read_macro(&mut self, slot: &str) -> Result<macros::Snapshot, String> {
        macro_adapter::read_with(&self.access, slot)
    }

    fn read_macro_catalog(&mut self, slots: &[String]) -> Result<Vec<macros::Snapshot>, String> {
        macro_adapter::read_catalog_with(&self.access, slots)
    }

    fn apply_macro(
        &mut self,
        expected: &macros::Snapshot,
        desired: &macros::Program,
        backup_dir: &Path,
    ) -> Result<macros::Snapshot, byakko_core::contract::ApplyFailure> {
        macro_adapter::apply_with(&self.access, expected, desired, backup_dir)
    }

    fn read_lighting(&mut self) -> Result<byakko_core::model::lighting::Snapshot, String> {
        crate::nia87::lighting_adapter::read_with(&self.access)
    }

    fn apply_lighting(
        &mut self,
        expected: &byakko_core::model::lighting::Snapshot,
        desired: &byakko_core::model::lighting::Setting,
        backup_dir: &Path,
    ) -> Result<byakko_core::model::lighting::Snapshot, byakko_core::contract::ApplyFailure> {
        crate::nia87::lighting_adapter::apply_with(&self.access, expected, desired, backup_dir)
    }

    fn start_host_lighting(
        &mut self,
        mode: byakko_core::model::lighting::HostMode,
        setting: Option<byakko_core::model::lighting::Setting>,
        expected: &byakko_core::model::lighting::Snapshot,
        backup_dir: &Path,
    ) -> Result<Box<dyn crate::HostActivity>, byakko_core::contract::ApplyFailure> {
        crate::nia87::host_adapter::start(&self.access, mode, setting, expected, backup_dir)
    }

    fn read_picture(&mut self) -> Result<byakko_core::model::picture::Snapshot, String> {
        crate::nia87::picture_adapter::read_with(&self.access)
    }

    fn apply_picture(
        &mut self,
        expected: &byakko_core::model::picture::Snapshot,
        desired: &BTreeMap<String, [u8; 3]>,
        backup_dir: &Path,
    ) -> Result<byakko_core::model::picture::Snapshot, byakko_core::contract::ApplyFailure> {
        crate::nia87::picture_adapter::apply_with(&self.access, expected, desired, backup_dir)
    }

    fn read_settings(&mut self) -> Result<byakko_core::model::settings::Snapshot, String> {
        crate::nia87::settings_adapter::read_with(&self.access)
    }

    fn apply_setting(
        &mut self,
        expected: &byakko_core::model::settings::Snapshot,
        edit: &byakko_core::model::settings::Edit,
        backup_dir: &Path,
    ) -> Result<byakko_core::model::settings::Snapshot, byakko_core::contract::ApplyFailure> {
        crate::nia87::settings_adapter::apply_with(&self.access, expected, edit, backup_dir)
    }

    fn archive_capabilities(&self) -> Option<byakko_core::model::archive::ArchiveCapabilities> {
        Some(byakko_protocol::nia87::archive_adapter::capabilities())
    }

    fn capture_archive(&mut self) -> Result<byakko_core::model::archive::NativeArchive, String> {
        crate::nia87::archive_adapter::capture_with(&self.access)
    }

    fn review_archive(
        &mut self,
        target: &byakko_core::model::archive::NativeArchive,
    ) -> Result<byakko_core::model::archive::Review, String> {
        crate::nia87::archive_adapter::review_with(&self.access, target)
    }

    fn apply_archive(
        &mut self,
        expected: &byakko_core::model::archive::NativeArchive,
        target: &byakko_core::model::archive::NativeArchive,
        backup_dir: &Path,
    ) -> Result<byakko_core::model::archive::NativeArchive, byakko_core::contract::ApplyFailure>
    {
        crate::nia87::archive_adapter::apply_with(&self.access, expected, target, backup_dir)
    }
}
