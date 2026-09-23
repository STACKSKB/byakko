//! Nia87 host-frame activity behind the portable device contract.
use crate::{
    HostActivity, HostFrame, HostMode,
    nia87::{device, lighting as native, lighting_adapter},
};
use byakko_core::{
    lighting::{self, Content, Snapshot},
    session::{ApplyFailure, Recovery},
};
use std::path::Path;

struct ScreenActivity {
    session: device::HostLightingSession,
    expected: Snapshot,
    backup_dir: std::path::PathBuf,
}

fn baseline(expected: &Snapshot) -> Result<native::Lighting, ApplyFailure> {
    let reject = |message: String| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    };
    if expected.backend_id != lighting_adapter::BACKEND_ID
        || !matches!(&expected.content, Content::Editable(_))
    {
        return Err(reject(
            "Read an editable Nia87 lighting baseline first".into(),
        ));
    }
    let raw = native::Lighting::decode(&expected.revision).map_err(reject)?;
    if lighting_adapter::from_native(&raw) != *expected {
        return Err(reject("Lighting baseline differs from its revision".into()));
    }
    Ok(raw)
}

pub(super) fn start(
    access: &device::Access,
    mode: HostMode,
    expected: &Snapshot,
    backup_dir: &Path,
) -> Result<Box<dyn HostActivity>, ApplyFailure> {
    let original = baseline(expected)?;
    let setting = match mode {
        HostMode::Screen => native::LightingSetting {
            effect_id: 21,
            value: None,
            speed: None,
            option: None,
            rgb: None,
            dazzle: false,
        },
    };
    let session = access.start_host_lighting_detailed(&original, &setting, backup_dir)?;
    Ok(Box::new(ScreenActivity {
        session,
        expected: expected.clone(),
        backup_dir: backup_dir.to_owned(),
    }))
}

impl HostActivity for ScreenActivity {
    fn send_frame(&mut self, frame: HostFrame) -> Result<(), String> {
        match frame {
            HostFrame::Rgb(rgb) => self
                .session
                .send_color(rgb)
                .map_err(|error| error.to_string()),
        }
    }

    fn finish(self: Box<Self>) -> Result<lighting::Snapshot, ApplyFailure> {
        let Self {
            session,
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
        if snapshot != expected {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_rejects_opaque_and_forged_values_before_access() {
        let mut raw = [0u8; 64];
        raw[0] = native::LED_READ_COMMAND;
        raw[1] = 21;
        let opaque = lighting_adapter::from_native(&native::Lighting::decode(&raw).unwrap());
        assert!(matches!(opaque.content, Content::Opaque { .. }));
        assert_eq!(
            baseline(&opaque).unwrap_err().recovery,
            Recovery::NotAttempted
        );
        let mut forged = opaque;
        forged.content = Content::Editable(lighting::Setting {
            effect: "1".into(),
            brightness: Some(4),
            speed: None,
            option: None,
            color: Some(lighting::Color::Rgb([255; 3])),
        });
        assert_eq!(
            baseline(&forged).unwrap_err().recovery,
            Recovery::NotAttempted
        );
    }
}
