use super::*;

#[derive(Clone, Copy)]
pub(super) enum Selection<'a> {
    Unique,
    Expected(&'a Target),
}

impl Selection<'_> {
    pub(super) fn open(self) -> Result<(Candidate, HidDevice)> {
        match self {
            Self::Unique => open_unique(),
            Self::Expected(target) => open_expected(target),
        }
    }
}

/// Immutable device-selection policy shared by adapters and feature reads.
/// `Unique` preserves the legacy CLI discovery behavior; `Bound` never falls
/// back from the selected HID collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Access {
    Unique,
    Bound(Target),
}

impl Access {
    pub fn unique() -> Self {
        Self::Unique
    }

    pub fn bound(target: Target) -> Self {
        Self::Bound(target)
    }

    fn selection(&self) -> Selection<'_> {
        match self {
            Self::Unique => Selection::Unique,
            Self::Bound(target) => Selection::Expected(target),
        }
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        keymaps::snapshot_with(self.selection())
    }

    pub fn read_macro(&self, slot: u8) -> Result<Vec<u8>> {
        macros::read_macro_with(self.selection(), slot)
    }

    pub fn read_picture(&self) -> Result<Vec<[u8; 3]>> {
        picture::read_picture_with(self.selection())
    }

    pub fn read_lighting(&self) -> Result<crate::nia87::lighting::Lighting> {
        lighting::read_lighting_with(self.selection())
    }

    pub fn read_settings(&self) -> Result<crate::nia87::settings::Settings> {
        settings::read_settings_with(self.selection())
    }

    pub fn apply_keymaps(
        &self,
        expected: &Snapshot,
        base: &[[u8; 4]],
        function: &[[u8; 4]],
        backup_dir: &std::path::Path,
    ) -> Result<Snapshot> {
        keymaps::apply_keymaps_with(self.selection(), expected, base, function, backup_dir)
    }

    pub fn apply_keymaps_detailed(
        &self,
        expected: &Snapshot,
        base: &[[u8; 4]],
        function: &[[u8; 4]],
        backup_dir: &std::path::Path,
    ) -> std::result::Result<Snapshot, byakko_core::session::ApplyFailure> {
        detailed(self.apply_keymaps(expected, base, function, backup_dir))
    }

    pub fn apply_macro(
        &self,
        slot: u8,
        expected: &[u8],
        new_macro: &crate::nia87::macros::Macro,
        backup_dir: &std::path::Path,
    ) -> Result<Vec<u8>> {
        macros::apply_macro_with(self.selection(), slot, expected, new_macro, backup_dir)
    }

    pub fn apply_macro_detailed(
        &self,
        slot: u8,
        expected: &[u8],
        new_macro: &crate::nia87::macros::Macro,
        backup_dir: &std::path::Path,
    ) -> std::result::Result<Vec<u8>, byakko_core::session::ApplyFailure> {
        detailed(self.apply_macro(slot, expected, new_macro, backup_dir))
    }

    pub fn apply_picture(
        &self,
        expected: &[[u8; 3]],
        desired: &[[u8; 3]],
        backup_dir: &std::path::Path,
    ) -> Result<Vec<[u8; 3]>> {
        picture::apply_picture_with(self.selection(), expected, desired, backup_dir)
    }

    pub fn apply_picture_detailed(
        &self,
        expected: &[[u8; 3]],
        desired: &[[u8; 3]],
        backup_dir: &std::path::Path,
    ) -> std::result::Result<Vec<[u8; 3]>, byakko_core::session::ApplyFailure> {
        detailed(self.apply_picture(expected, desired, backup_dir))
    }

    pub fn apply_lighting(
        &self,
        expected: &crate::nia87::lighting::Lighting,
        setting: &crate::nia87::lighting::LightingSetting,
        backup_dir: &std::path::Path,
    ) -> Result<crate::nia87::lighting::Lighting> {
        lighting::apply_lighting_with(self.selection(), expected, setting, backup_dir)
    }

    pub fn apply_lighting_detailed(
        &self,
        expected: &crate::nia87::lighting::Lighting,
        setting: &crate::nia87::lighting::LightingSetting,
        backup_dir: &std::path::Path,
    ) -> std::result::Result<crate::nia87::lighting::Lighting, byakko_core::session::ApplyFailure>
    {
        detailed(self.apply_lighting(expected, setting, backup_dir))
    }

    pub fn apply_setting(
        &self,
        expected: &crate::nia87::settings::Settings,
        setting: crate::nia87::settings::Setting,
        backup_dir: &std::path::Path,
    ) -> Result<crate::nia87::settings::Settings> {
        settings::apply_setting_with(self.selection(), expected, setting, backup_dir)
    }

    pub fn apply_setting_detailed(
        &self,
        expected: &crate::nia87::settings::Settings,
        setting: crate::nia87::settings::Setting,
        backup_dir: &std::path::Path,
    ) -> std::result::Result<crate::nia87::settings::Settings, byakko_core::session::ApplyFailure>
    {
        detailed(self.apply_setting(expected, setting, backup_dir))
    }

    pub fn capture_configuration(
        &self,
        progress: impl FnMut(usize, usize),
    ) -> Result<crate::nia87::configuration::Configuration> {
        configuration::capture_selected(self.selection(), progress)
    }

    pub fn apply_configuration_detailed(
        &self,
        expected: &crate::nia87::configuration::Configuration,
        target: &crate::nia87::configuration::Configuration,
        backup_dir: &std::path::Path,
        progress: impl FnMut(&str),
    ) -> std::result::Result<
        crate::nia87::configuration::Configuration,
        byakko_core::session::ApplyFailure,
    > {
        configuration::apply_detailed_selected(
            self.selection(),
            expected,
            target,
            backup_dir,
            progress,
        )
    }
}
