//! Nia87 host-frame activity behind the portable device contract.
use crate::{
    HostActivity, HostFrame,
    nia87::{device, lighting as native, lighting_adapter},
};
use byakko_core::{
    lighting::{self, Content, HostMode, HostSource, Snapshot},
    session::{ApplyFailure, Recovery},
};
use std::path::Path;

struct NiaHostActivity {
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
    let setting = native_mode(&mode)?;
    let session = access.start_host_lighting_detailed(&original, &setting, backup_dir)?;
    Ok(Box::new(NiaHostActivity {
        session,
        expected: expected.clone(),
        backup_dir: backup_dir.to_owned(),
    }))
}

fn native_mode(mode: &HostMode) -> Result<native::LightingSetting, ApplyFailure> {
    let setting = match (mode.id.as_str(), mode.source) {
        ("screen-average", HostSource::ScreenAverage) => native::LightingSetting {
            effect_id: 21,
            value: None,
            speed: None,
            option: None,
            rgb: None,
            dazzle: false,
        },
        ("music-follow-2" | "music-follow-3", HostSource::PlaybackAudio { bands: 32 }) => {
            native::LightingSetting {
                effect_id: if mode.id == "music-follow-2" { 22 } else { 20 },
                value: Some(4),
                speed: None,
                option: Some(0),
                rgb: Some([0, 255, 0]),
                dazzle: false,
            }
        }
        _ => {
            return Err(ApplyFailure {
                message: "Host lighting mode is unavailable on Nia87".into(),
                recovery: Recovery::NotAttempted,
            });
        }
    };
    Ok(setting)
}

impl HostActivity for NiaHostActivity {
    fn send_frame(&mut self, frame: HostFrame) -> Result<(), String> {
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

    #[test]
    fn advertised_music_modes_select_distinct_native_effects() {
        let modes = lighting_adapter::capabilities().host_modes;
        for (id, effect) in [("music-follow-2", 22), ("music-follow-3", 20)] {
            let mode = modes.iter().find(|mode| mode.id == id).unwrap();
            let setting = native_mode(mode).unwrap();
            assert_eq!(setting.effect_id, effect);
            assert_eq!(setting.value, Some(4));
            assert_eq!(setting.option, Some(0));
            assert_eq!(setting.rgb, Some([0, 255, 0]));
            assert_eq!(native::write_report(&setting).unwrap()[1], effect);
        }
        let unsupported = HostMode {
            id: "other-backend-mode".into(),
            label: "Other".into(),
            source: HostSource::PlaybackAudio { bands: 32 },
        };
        assert_eq!(
            native_mode(&unsupported).unwrap_err().recovery,
            Recovery::NotAttempted
        );
    }
}
