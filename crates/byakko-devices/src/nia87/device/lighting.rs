use super::apply_error::{ApplyError, lighting_apply_error};
use super::transaction::{VerifiedStep, apply_roundtrip, pacing, save_json_backup};
use super::*;
use byakko_core::session::{ApplyFailure, Recovery};

/// Read one raw-preserving global-lighting response.
pub fn read_lighting() -> Result<crate::nia87::lighting::Lighting> {
    read_lighting_with(Selection::Unique)
}

pub(super) fn read_lighting_with(
    selection: Selection<'_>,
) -> Result<crate::nia87::lighting::Lighting> {
    let session = Session::open_for(selection)?;
    read_lighting_on_device(session.device())
}

/// Exclusive host-lighting session. The lock covers setup, every frame, and
/// restoration. Explicit finish reports restoration errors to the caller.
pub struct HostLightingSession {
    session: Session,
    target: Target,
    saved: crate::nia87::lighting::Lighting,
    active: crate::nia87::lighting::Lighting,
    backups: std::path::PathBuf,
    finished: bool,
}

pub type ScreenSession = HostLightingSession;

#[derive(Clone, Copy)]
enum CompletionPolicy {
    VerifiedReadback,
    TransportAccepted,
}

impl HostLightingSession {
    pub fn start(
        expected: &crate::nia87::lighting::Lighting,
        backups: &std::path::Path,
    ) -> Result<Self> {
        let desired = crate::nia87::lighting::LightingSetting {
            effect_id: 21,
            value: None,
            speed: None,
            option: None,
            rgb: None,
            dazzle: false,
        };
        Self::start_mode(expected, &desired, backups)
    }

    pub fn start_mode(
        expected: &crate::nia87::lighting::Lighting,
        desired: &crate::nia87::lighting::LightingSetting,
        backups: &std::path::Path,
    ) -> Result<Self> {
        Self::start_mode_with(Selection::Unique, expected, desired, backups)
    }

    pub(super) fn start_mode_with(
        selection: Selection<'_>,
        expected: &crate::nia87::lighting::Lighting,
        desired: &crate::nia87::lighting::LightingSetting,
        backups: &std::path::Path,
    ) -> Result<Self> {
        if !matches!(desired.effect_id, 20..=22) {
            return Err("Host lighting requires screen or music mode".into());
        }
        let session = Session::open_for(selection)?;
        let target = session.target()?;
        if !read_settings_on_device(session.device())?.backlight_enabled() {
            return Err("Enable the backlight in Settings before starting host lighting".into());
        }
        let active = apply_lighting_unlocked(
            Selection::Expected(&target),
            expected,
            desired,
            backups,
            CompletionPolicy::VerifiedReadback,
        )?;
        Ok(Self {
            session,
            target,
            saved: expected.clone(),
            active,
            backups: backups.to_owned(),
            finished: false,
        })
    }

    pub fn send_color(&self, rgb: [u8; 3]) -> Result<()> {
        if self.active.effect_id() != 21 {
            return Err("Screen frame requires screen mode".into());
        }
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&crate::nia87::host_lighting::screen_report(rgb));
        self.session.device().send_setter(&host)?;
        Ok(())
    }

    pub fn send_music(&self, bands: [u8; 32]) -> Result<()> {
        if !matches!(self.active.effect_id(), 20 | 22) {
            return Err("Music frame requires music mode".into());
        }
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&crate::nia87::host_lighting::music_report(bands));
        self.session.device().send_setter(&host)?;
        Ok(())
    }

    fn restore(&mut self) -> Result<crate::nia87::lighting::Lighting> {
        let setting = self
            .saved
            .recognized_setting()
            .ok_or("Unrecognized saved lighting")?;
        let restored = apply_lighting_unlocked(
            Selection::Expected(&self.target),
            &self.active,
            &setting,
            &self.backups,
            CompletionPolicy::VerifiedReadback,
        )?;
        self.finished = true;
        Ok(restored)
    }

    pub fn finish(mut self) -> Result<crate::nia87::lighting::Lighting> {
        let result = self.restore();
        // Do not silently repeat a failed write during Drop; report it to the UI.
        self.finished = true;
        result
    }
}

impl Drop for HostLightingSession {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.restore();
        }
    }
}

pub(super) fn read_lighting_on_device(
    device: &HidDevice,
) -> Result<crate::nia87::lighting::Lighting> {
    let response = read_payload(device, crate::nia87::lighting::LED_READ_COMMAND, 0, 0)?;
    if response[0] != crate::nia87::lighting::LED_READ_COMMAND {
        return Err("Lighting read returned an unrelated opcode".into());
    }
    Ok(crate::nia87::lighting::Lighting::decode(&response)?)
}

/// Rebuild only the global setting bytes exposed by PB's LED writer. The
/// unknown response tail remains in the backup, but is not sent as an
/// undocumented command payload during restoration.
pub(super) fn lighting_restore_report(original: &crate::nia87::lighting::Lighting) -> [u8; 64] {
    let mut report = [0u8; 64];
    report[0] = crate::nia87::lighting::LED_WRITE_COMMAND;
    report[1..8].copy_from_slice(&original.raw()[1..8]);
    let sum = report[..8]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[8] = 0xffu8.wrapping_sub(sum);
    report
}

pub(super) fn write_lighting_report(device: &HidDevice, report: &[u8; 64]) -> Result<()> {
    let mut host = [0u8; 65];
    host[1..].copy_from_slice(report);
    device.send_setter(&host)?;
    std::thread::sleep(pacing::LIGHTING_SETTER);
    Ok(())
}

pub(super) fn lighting_matches_report(
    actual: &crate::nia87::lighting::Lighting,
    report: &[u8; 64],
    original: &crate::nia87::lighting::Lighting,
) -> bool {
    actual.raw()[1..8] == report[1..8] && actual.raw()[9..] == original.raw()[9..]
}

/// Submit ordinary global lighting with a durable raw backup. A successful
/// return means the transport accepted the setter, not that a getter verified
/// firmware persistence. Host start and restore use the verified path below.
pub fn apply_lighting(
    expected: &crate::nia87::lighting::Lighting,
    setting: &crate::nia87::lighting::LightingSetting,
    backup_dir: &std::path::Path,
) -> Result<crate::nia87::lighting::Lighting> {
    apply_lighting_with(Selection::Unique, expected, setting, backup_dir)
}

pub(super) fn apply_lighting_with(
    selection: Selection<'_>,
    expected: &crate::nia87::lighting::Lighting,
    setting: &crate::nia87::lighting::LightingSetting,
    backup_dir: &std::path::Path,
) -> Result<crate::nia87::lighting::Lighting> {
    let _lock = transaction_lock()?;
    apply_lighting_unlocked(
        selection,
        expected,
        setting,
        backup_dir,
        CompletionPolicy::TransportAccepted,
    )
}

pub fn apply_lighting_detailed(
    expected: &crate::nia87::lighting::Lighting,
    setting: &crate::nia87::lighting::LightingSetting,
    backup_dir: &std::path::Path,
) -> std::result::Result<crate::nia87::lighting::Lighting, byakko_core::session::ApplyFailure> {
    detailed(apply_lighting(expected, setting, backup_dir))
}

fn apply_lighting_unlocked(
    selection: Selection<'_>,
    expected: &crate::nia87::lighting::Lighting,
    setting: &crate::nia87::lighting::LightingSetting,
    backup_dir: &std::path::Path,
    policy: CompletionPolicy,
) -> Result<crate::nia87::lighting::Lighting> {
    if expected.raw()[0] != crate::nia87::lighting::LED_READ_COMMAND
        || expected.recognized_setting().is_none()
    {
        return Err("Lighting baseline is not a recognized Nia87 LED response".into());
    }
    let target = crate::nia87::lighting::write_report(setting)?;
    let (_, device) = selection.open()?;
    if lighting_matches_report(expected, &target, expected) {
        return Ok(expected.clone());
    }

    let backup = save_json_backup(
        backup_dir,
        "lighting-before",
        &serde_json::json!({
            "format_version": 1,
            "firmware": 0x0100,
            "profile": 0,
            "before": expected,
            "target_report": target.as_slice(),
        }),
    )?;

    if matches!(policy, CompletionPolicy::TransportAccepted) {
        // The captured official UI updates its cache after the setter without
        // a getter. Keep transport acceptance distinct from verified reads.
        submit_lighting_report(&target, backup.path(), |report| {
            write_lighting_report(&device, report)
        })?;
        return submitted_lighting(expected, &target);
    }

    let restore_report = lighting_restore_report(expected);
    apply_roundtrip(
        &backup,
        VerifiedStep {
            write: || write_lighting_report(&device, &target),
            matches: |actual: &crate::nia87::lighting::Lighting| {
                lighting_matches_report(actual, &target, expected)
            },
            mismatch: "Lighting readback differs in setting or reserved response bytes",
        },
        VerifiedStep {
            write: || write_lighting_report(&device, &restore_report),
            matches: |actual: &crate::nia87::lighting::Lighting| {
                lighting_matches_report(actual, &restore_report, expected)
            },
            mismatch: "Lighting restoration could not be verified",
        },
        || read_lighting_on_device(&device),
        None,
        lighting_apply_error,
    )
}

fn submitted_lighting(
    expected: &crate::nia87::lighting::Lighting,
    report: &[u8; 64],
) -> Result<crate::nia87::lighting::Lighting> {
    let mut submitted = expected.raw().to_vec();
    submitted[1..8].copy_from_slice(&report[1..8]);
    Ok(crate::nia87::lighting::Lighting::decode(&submitted)?)
}

fn submit_lighting_report(
    report: &[u8; 64],
    backup: &std::path::Path,
    mut send: impl FnMut(&[u8; 64]) -> Result<()>,
) -> Result<()> {
    send(report).map_err(|error| {
        ApplyError(ApplyFailure {
            message: format!(
                "Lighting upload stopped after a transport error: {error}. Device state is unknown; no automatic restore sent. Backup: {}",
                backup.display()
            ),
            recovery: Recovery::Unverified,
        })
        .into()
    })
}

#[cfg(test)]
mod submission_tests {
    use super::*;

    #[test]
    fn ordinary_submission_sends_once_without_read_or_restore() {
        let report = [7; 64];
        let backup = std::path::Path::new("lighting-before.json");
        let mut sends = 0;
        submit_lighting_report(&report, backup, |sent| {
            sends += 1;
            assert_eq!(sent, &report);
            Ok(())
        })
        .unwrap();
        assert_eq!(sends, 1);

        let failure = detailed(submit_lighting_report(&report, backup, |_| {
            sends += 1;
            Err("Disconnected".into())
        }))
        .unwrap_err();
        assert_eq!(sends, 2);
        assert_eq!(failure.recovery, Recovery::Unverified);
        assert!(failure.message.contains("Disconnected"));
        assert!(failure.message.contains("no automatic restore"));
    }

    #[test]
    fn submitted_revision_changes_only_known_setting_bytes() {
        let mut original = [0xa5; 64];
        original[..8].copy_from_slice(&[0x87, 1, 4, 4, 7, 1, 2, 3]);
        let expected = crate::nia87::lighting::Lighting::decode(&original).unwrap();
        let mut report = [0; 64];
        report[..8].copy_from_slice(&[0x07, 2, 3, 2, 8, 9, 8, 7]);
        let submitted = submitted_lighting(&expected, &report).unwrap();
        assert_eq!(submitted.raw()[0], 0x87);
        assert_eq!(&submitted.raw()[1..8], &report[1..8]);
        assert_eq!(&submitted.raw()[8..], &original[8..]);
    }
}
