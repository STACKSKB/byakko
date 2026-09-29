//! Nia87 host-frame activity behind the portable device contract.
use crate::{HostActivity, HostFrame, nia87::device};
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    editor::{Feature, lighting::LightingRules},
    model::lighting::{self, HostMode, HostSource, Setting, Snapshot},
};
use byakko_protocol::nia87::{host_adapter, lighting_adapter};
use std::path::Path;

struct NiaHostActivity {
    session: device::HostLightingSession,
    mode: HostMode,
    source: HostSource,
    expected: Snapshot,
    backup_dir: std::path::PathBuf,
}

pub(super) fn start(
    access: &device::Access,
    mode: HostMode,
    setting: Option<Setting>,
    expected: &Snapshot,
    backup_dir: &Path,
) -> Result<Box<dyn HostActivity>, ApplyFailure> {
    let original = host_adapter::baseline(expected)?;
    let native_setting = host_adapter::native_mode(&mode, setting.as_ref())?;
    let session = access.start_host_lighting(&original, &native_setting, backup_dir)?;
    Ok(Box::new(NiaHostActivity {
        session,
        source: mode.source,
        mode,
        expected: expected.clone(),
        backup_dir: backup_dir.to_owned(),
    }))
}

impl HostActivity for NiaHostActivity {
    fn send_frame(&mut self, frame: HostFrame) -> Result<(), String> {
        frame.validate_for(self.source)?;
        match frame {
            HostFrame::Rgb(rgb) => self
                .session
                .send_color(rgb)
                .map_err(|error| error.to_string()),
            HostFrame::Bands(bands) => self
                .session
                .send_music(
                    bands
                        .try_into()
                        .map_err(|_| "Nia87 requires 32 audio bands")?,
                )
                .map_err(|error| error.to_string()),
        }
    }

    fn update_parameters(&mut self, setting: Setting) -> Result<(), String> {
        let native = host_adapter::native_mode(&self.mode, Some(&setting))
            .map_err(|failure| failure.message)?;
        self.session
            .update_parameters(&native)
            .map_err(|error| error.to_string())
    }
    fn finish(self: Box<Self>) -> Result<lighting::Snapshot, ApplyFailure> {
        let Self {
            session,
            source: _,
            mode: _,
            expected,
            backup_dir,
        } = *self;
        let restored = session.finish().map_err(|error| ApplyFailure {
            message: format!(
                "Host lighting restoration was not verified: {error}. Backups: {}",
                backup_dir.display()
            ),
            recovery: Recovery::Unverified,
        })?;
        let snapshot = lighting_adapter::from_native(&restored);
        if !LightingRules::same_baseline(&snapshot, &expected) {
            return Err(ApplyFailure {
                message: format!(
                    "Host lighting readback differs from the saved baseline. Backups: {}",
                    backup_dir.display()
                ),
                recovery: Recovery::Unverified,
            });
        }
        Ok(snapshot)
    }
}
