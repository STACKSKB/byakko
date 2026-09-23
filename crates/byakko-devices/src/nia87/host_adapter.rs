//! Nia87 host-frame activity behind the portable device contract.
use crate::{
    HostActivity, HostFrame,
    nia87::{device, lighting as native, lighting_adapter},
};
use byakko_core::{
    lighting::{self, Color, Content, HostMode, HostSource, Setting, Snapshot},
    session::{ApplyFailure, Recovery},
};
use std::path::Path;

struct NiaHostActivity {
    session: device::HostLightingSession,
    source: HostSource,
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
    setting: Option<Setting>,
    expected: &Snapshot,
    backup_dir: &Path,
) -> Result<Box<dyn HostActivity>, ApplyFailure> {
    let original = baseline(expected)?;
    let native_setting = native_mode(&mode, setting.as_ref())?;
    let session = access.start_host_lighting_detailed(&original, &native_setting, backup_dir)?;
    Ok(Box::new(NiaHostActivity {
        session,
        source: mode.source,
        expected: expected.clone(),
        backup_dir: backup_dir.to_owned(),
    }))
}

fn native_mode(
    mode: &HostMode,
    parameters: Option<&Setting>,
) -> Result<native::LightingSetting, ApplyFailure> {
    let reject = |message: String| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    };
    let catalog = lighting_adapter::capabilities();
    let offered = catalog
        .host_modes
        .iter()
        .find(|offered| offered.id == mode.id)
        .filter(|offered| *offered == mode)
        .ok_or_else(|| reject("Host lighting mode is unavailable on Nia87".into()))?;
    match (&offered.parameters, parameters) {
        (None, None) => {}
        (Some(schema), Some(setting)) => {
            lighting::validate_parameters(&schema.schema, setting).map_err(reject)?
        }
        _ => {
            return Err(reject(
                "Host lighting parameters do not match this mode".into(),
            ));
        }
    }
    let setting = match (mode.id.as_str(), mode.source, parameters) {
        ("screen-average", HostSource::ScreenAverage, None) => native::LightingSetting {
            effect_id: 21,
            value: None,
            speed: None,
            option: None,
            rgb: None,
            dazzle: false,
        },
        (
            "music-follow-2" | "music-follow-3",
            HostSource::PlaybackAudio { bands: 32 },
            Some(parameters),
        ) => {
            let effect_id = if mode.id == "music-follow-2" { 22 } else { 20 };
            let effect = native::effect_by_id(effect_id).expect("known music effect");
            let option = parameters
                .option
                .as_ref()
                .and_then(|id| effect.options.iter().position(|item| item == id));
            let (rgb, dazzle) = match parameters.color.as_ref() {
                Some(Color::Rgb(rgb)) => (Some(*rgb), false),
                Some(Color::Rainbow) => (Some([255; 3]), true),
                None => return Err(reject("Music color is required".into())),
            };
            native::LightingSetting {
                effect_id,
                value: parameters.brightness.map(|value| value as u8),
                speed: None,
                option: option.map(|index| index as u8),
                rgb,
                dazzle,
            }
        }
        _ => return Err(reject("Host lighting mode is unavailable on Nia87".into())),
    };
    native::write_report(&setting).map_err(reject)?;
    Ok(setting)
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

    fn finish(self: Box<Self>) -> Result<lighting::Snapshot, ApplyFailure> {
        let Self {
            session,
            source: _,
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
            let setting = native_mode(
                mode,
                mode.parameters
                    .as_ref()
                    .map(|parameters| &parameters.default),
            )
            .unwrap();
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
            parameters: None,
        };
        assert_eq!(
            native_mode(&unsupported, None).unwrap_err().recovery,
            Recovery::NotAttempted
        );
    }

    #[test]
    fn music_parameters_encode_each_option_brightness_and_color_mode() {
        for mode in lighting_adapter::capabilities()
            .host_modes
            .into_iter()
            .filter(|mode| mode.parameters.is_some())
        {
            let mut setting = mode.parameters.as_ref().unwrap().default.clone();
            for brightness in [0, 4] {
                for (option, index) in [("upright", 0), ("separate", 1), ("intersect", 2)] {
                    setting.brightness = Some(brightness);
                    setting.option = Some(option.into());
                    setting.color = Some(Color::Rgb([7, 8, 9]));
                    let native = native_mode(&mode, Some(&setting)).unwrap();
                    let report = native::write_report(&native).unwrap();
                    assert_eq!(report[3], brightness as u8);
                    assert_eq!(report[4], (index << 4) | 4);
                    assert_eq!(&report[5..8], &[7, 8, 9]);
                }
            }
            setting.color = Some(Color::Rainbow);
            let report =
                native::write_report(&native_mode(&mode, Some(&setting)).unwrap()).unwrap();
            assert_eq!(report[4], 2 << 4);
            setting.brightness = Some(5);
            assert_eq!(
                native_mode(&mode, Some(&setting)).unwrap_err().recovery,
                Recovery::NotAttempted
            );
            assert_eq!(
                native_mode(&mode, None).unwrap_err().recovery,
                Recovery::NotAttempted
            );
            let mut forged = mode.clone();
            forged.source = HostSource::ScreenAverage;
            assert_eq!(
                native_mode(&forged, Some(&setting)).unwrap_err().recovery,
                Recovery::NotAttempted
            );
        }
    }
}
