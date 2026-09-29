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

/// Immutable selected HID collection shared by adapters and feature reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    target: Target,
}

impl Access {
    pub fn bound(target: Target) -> Self {
        Self { target }
    }

    fn selection(&self) -> Selection<'_> {
        Selection::Expected(&self.target)
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        keymaps::snapshot_with(self.selection())
    }

    pub fn read_macro(&self, slot: u8) -> Result<Vec<u8>> {
        macros::read_macro_with(self.selection(), slot)
    }

    pub fn read_macros(&self, slots: &[u8]) -> Result<Vec<Vec<u8>>> {
        macros::read_macros_with(self.selection(), slots)
    }

    pub fn read_picture_with_context(&self) -> Result<(Vec<[u8; 3]>, [u8; 2])> {
        picture::read_picture_with_context(self.selection())
    }

    pub fn read_lighting(&self) -> Result<byakko_protocol::nia87::lighting::Lighting> {
        lighting::read_lighting_with(self.selection())
    }

    pub fn read_settings(&self) -> Result<byakko_protocol::nia87::settings::Settings> {
        settings::read_settings_with(self.selection())
    }

    pub fn apply_keymaps(
        &self,
        expected: &Snapshot,
        base: &[[u8; 4]],
        function: &[[u8; 4]],
        backup_dir: &std::path::Path,
    ) -> ApplyResult<Snapshot> {
        keymaps::apply_keymaps_with(self.selection(), expected, base, function, backup_dir)
    }

    pub fn apply_macro(
        &self,
        slot: u8,
        expected: &[u8],
        new_macro: &byakko_protocol::nia87::macros::Macro,
        backup_dir: &std::path::Path,
    ) -> ApplyResult<Vec<u8>> {
        macros::apply_macro_with(self.selection(), slot, expected, new_macro, backup_dir)
    }

    /// Use a before-image already validated by the portable macro adapter.
    pub fn apply_macro_validated(
        &self,
        slot: u8,
        expected: &byakko_protocol::nia87::macros::ValidatedBeforeImage,
        new_macro: &byakko_protocol::nia87::macros::Macro,
        backup_dir: &std::path::Path,
    ) -> ApplyResult<Vec<u8>> {
        macros::apply_macro_validated_with(self.selection(), slot, expected, new_macro, backup_dir)
    }

    pub fn apply_picture(
        &self,
        expected: &[[u8; 3]],
        desired: &[[u8; 3]],
        expected_context: [u8; 2],
        backup_dir: &std::path::Path,
    ) -> ApplyResult<Vec<[u8; 3]>> {
        picture::apply_picture_with(
            self.selection(),
            expected,
            desired,
            Some(expected_context),
            backup_dir,
        )
    }

    pub fn apply_lighting(
        &self,
        expected: &byakko_protocol::nia87::lighting::Lighting,
        setting: &byakko_protocol::nia87::lighting::LightingSetting,
        backup_dir: &std::path::Path,
    ) -> ApplyResult<byakko_protocol::nia87::lighting::Lighting> {
        lighting::apply_lighting_with(self.selection(), expected, setting, backup_dir)
    }

    pub fn start_host_lighting(
        &self,
        expected: &byakko_protocol::nia87::lighting::Lighting,
        setting: &byakko_protocol::nia87::lighting::LightingSetting,
        backup_dir: &std::path::Path,
    ) -> ApplyResult<lighting::HostLightingSession> {
        lighting::HostLightingSession::start_mode_with(
            self.selection(),
            expected,
            setting,
            backup_dir,
        )
    }

    pub fn apply_setting(
        &self,
        expected: &byakko_protocol::nia87::settings::Settings,
        setting: byakko_protocol::nia87::settings::Setting,
        backup_dir: &std::path::Path,
    ) -> ApplyResult<byakko_protocol::nia87::settings::Settings> {
        settings::apply_setting_with(self.selection(), expected, setting, backup_dir)
    }

    pub fn capture_configuration(
        &self,
        progress: impl FnMut(usize, usize),
    ) -> Result<byakko_protocol::nia87::configuration::Configuration> {
        configuration::capture_selected(self.selection(), progress)
    }

    pub fn apply_configuration(
        &self,
        expected: &byakko_protocol::nia87::configuration::Configuration,
        target: &byakko_protocol::nia87::configuration::Configuration,
        backup_dir: &std::path::Path,
        progress: impl FnMut(&str),
    ) -> ApplyResult<byakko_protocol::nia87::configuration::Configuration> {
        configuration::apply_configuration_selected(
            self.selection(),
            expected,
            target,
            backup_dir,
            progress,
        )
    }
}
