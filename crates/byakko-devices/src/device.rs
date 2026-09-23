//! Portable device contract; firmware and OS effects live in implementations.
use byakko_core::{
    Change, State, archive, lighting, macros, picture,
    session::{ApplyFailure, Recovery},
    settings,
};
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostFrame {
    Rgb([u8; 3]),
    Bands(Vec<u8>),
}

impl HostFrame {
    pub fn validate_for(&self, source: lighting::HostSource) -> Result<(), String> {
        match (source, self) {
            (lighting::HostSource::ScreenAverage, Self::Rgb(_)) => Ok(()),
            (lighting::HostSource::PlaybackAudio { bands }, Self::Bands(values))
                if values.len() == usize::from(bands) =>
            {
                Ok(())
            }
            (lighting::HostSource::PlaybackAudio { bands }, Self::Bands(_)) => {
                Err(format!("Audio frame must contain {bands} bands"))
            }
            (lighting::HostSource::ScreenAverage, Self::Bands(_)) => {
                Err("Screen-average mode requires an RGB frame".into())
            }
            (lighting::HostSource::PlaybackAudio { .. }, Self::Rgb(_)) => {
                Err("Playback-audio mode requires a band frame".into())
            }
        }
    }
}

/// A temporary effect owned by the device executor. `finish` restores and
/// verifies the saved lighting; Drop must attempt restoration on unwinding.
pub trait HostActivity: Send {
    fn send_frame(&mut self, frame: HostFrame) -> Result<(), String>;
    fn finish(self: Box<Self>) -> Result<lighting::Snapshot, ApplyFailure>;
}

/// Implementations must validate expected state, back up, write and verify.
/// Success means verified device state, not merely successful transmission.
pub trait Device: Send + 'static {
    fn read(&mut self) -> Result<State, String>;
    fn apply(
        &mut self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, ApplyFailure>;

    fn read_macro(&mut self, _slot: &str) -> Result<macros::Snapshot, String> {
        Err("Macro operations are unsupported by this device".into())
    }

    /// Read every advertised slot within one serialized executor operation.
    fn read_macro_catalog(&mut self, slots: &[String]) -> Result<Vec<macros::Snapshot>, String> {
        slots.iter().map(|slot| self.read_macro(slot)).collect()
    }

    fn apply_macro(
        &mut self,
        _expected: &macros::Snapshot,
        _desired: &macros::Program,
        _backup_dir: &Path,
    ) -> Result<macros::Snapshot, ApplyFailure> {
        Err(ApplyFailure {
            message: "Macro operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    fn read_lighting(&mut self) -> Result<lighting::Snapshot, String> {
        Err("Lighting operations are unsupported by this device".into())
    }

    fn apply_lighting(
        &mut self,
        _expected: &lighting::Snapshot,
        _desired: &lighting::Setting,
        _backup_dir: &Path,
    ) -> Result<lighting::Snapshot, ApplyFailure> {
        Err(ApplyFailure {
            message: "Lighting operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    /// A failed start must recover any mutation before returning its typed
    /// failure. Success transfers restoration ownership to the executor.
    fn start_host_lighting(
        &mut self,
        _mode: lighting::HostMode,
        _setting: Option<lighting::Setting>,
        _expected: &lighting::Snapshot,
        _backup_dir: &Path,
    ) -> Result<Box<dyn HostActivity>, ApplyFailure> {
        Err(ApplyFailure {
            message: "Host lighting is unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    fn read_picture(&mut self) -> Result<picture::Snapshot, String> {
        Err("Picture operations are unsupported by this device".into())
    }

    fn apply_picture(
        &mut self,
        _expected: &picture::Snapshot,
        _desired: &std::collections::BTreeMap<String, [u8; 3]>,
        _backup_dir: &Path,
    ) -> Result<picture::Snapshot, ApplyFailure> {
        Err(ApplyFailure {
            message: "Picture operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    fn read_settings(&mut self) -> Result<settings::Snapshot, String> {
        Err("Settings operations are unsupported by this device".into())
    }

    fn apply_setting(
        &mut self,
        _expected: &settings::Snapshot,
        _edit: &settings::Edit,
        _backup_dir: &Path,
    ) -> Result<settings::Snapshot, ApplyFailure> {
        Err(ApplyFailure {
            message: "Settings operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    fn archive_capabilities(&self) -> Option<archive::ArchiveCapabilities> {
        None
    }

    fn capture_archive(&mut self) -> Result<archive::NativeArchive, String> {
        Err("Native archive operations are unsupported by this device".into())
    }

    fn review_archive(
        &mut self,
        _target: &archive::NativeArchive,
    ) -> Result<archive::Review, String> {
        Err("Native archive operations are unsupported by this device".into())
    }

    fn apply_archive(
        &mut self,
        _expected: &archive::NativeArchive,
        _target: &archive::NativeArchive,
        _backup_dir: &Path,
    ) -> Result<archive::NativeArchive, ApplyFailure> {
        Err(ApplyFailure {
            message: "Native archive operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }
}

pub use Device as KeymapDevice;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_frames_match_the_selected_source_and_band_count() {
        let screen = lighting::HostSource::ScreenAverage;
        let music = lighting::HostSource::PlaybackAudio { bands: 32 };
        let rgb = HostFrame::Rgb([4, 5, 6]);
        let bands = HostFrame::Bands(vec![0; 32]);
        assert!(rgb.validate_for(screen).is_ok());
        assert!(bands.validate_for(music).is_ok());
        assert_eq!(
            bands.validate_for(screen).unwrap_err(),
            "Screen-average mode requires an RGB frame"
        );
        assert_eq!(
            rgb.validate_for(music).unwrap_err(),
            "Playback-audio mode requires a band frame"
        );
        assert_eq!(
            HostFrame::Bands(vec![0; 31])
                .validate_for(music)
                .unwrap_err(),
            "Audio frame must contain 32 bands"
        );
    }
}
