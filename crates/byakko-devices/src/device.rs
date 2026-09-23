//! Portable device contract; firmware and OS effects live in implementations.
use byakko_core::{
    Change, State, archive, lighting, macros, picture,
    session::{ApplyFailure, Recovery},
    settings,
};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostMode {
    Screen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostFrame {
    Rgb([u8; 3]),
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
        _mode: HostMode,
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
