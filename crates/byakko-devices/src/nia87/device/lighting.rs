use super::apply_error::{ApplyResult, lighting_apply_error, not_attempted};
use super::transaction::{VerifiedStep, apply_roundtrip, pacing, save_json_backup};
use super::*;
use byakko_core::contract::{ApplyFailure, Recovery};
use byakko_protocol::nia87::host_adapter::{
    lighting_matches_report, lighting_restore_report, submitted_lighting,
};

/// Read one raw-preserving global-lighting response.
pub fn read_lighting() -> Result<byakko_protocol::nia87::lighting::Lighting> {
    read_lighting_with(Selection::Unique)
}

pub(super) fn read_lighting_with(
    selection: Selection<'_>,
) -> Result<byakko_protocol::nia87::lighting::Lighting> {
    let session = Session::open_for(selection)?;
    read_lighting_on_device(session.device())
}

/// Exclusive host-lighting session. The lock covers setup, every frame, and
/// restoration. Explicit finish reports restoration errors to the caller.
pub struct HostLightingSession {
    session: Session,
    saved: byakko_protocol::nia87::lighting::Lighting,
    active: byakko_protocol::nia87::lighting::Lighting,
    backups: std::path::PathBuf,
    finished: bool,
}

#[derive(Clone, Copy)]
enum CompletionPolicy {
    VerifiedReadback,
    TransportAccepted,
}

impl HostLightingSession {
    pub(super) fn start_mode_with(
        selection: Selection<'_>,
        expected: &byakko_protocol::nia87::lighting::Lighting,
        desired: &byakko_protocol::nia87::lighting::LightingSetting,
        backups: &std::path::Path,
    ) -> ApplyResult<Self> {
        if !matches!(desired.effect_id, 20..=22) {
            return Err(not_attempted("Host lighting requires screen or music mode"));
        }
        let session = Session::open_for(selection).map_err(not_attempted)?;
        let active = apply_lighting_on_device(
            session.device(),
            expected,
            desired,
            backups,
            CompletionPolicy::VerifiedReadback,
        )?;
        Ok(Self {
            session,
            saved: expected.clone(),
            active,
            backups: backups.to_owned(),
            finished: false,
        })
    }

    pub fn update_parameters(
        &mut self,
        setting: &byakko_protocol::nia87::lighting::LightingSetting,
    ) -> Result<()> {
        if setting.effect_id != self.active.effect_id() || !matches!(setting.effect_id, 20 | 22) {
            return Err("Parameter update must retain the active music mode".into());
        }
        let report = byakko_protocol::nia87::lighting::write_report(setting)?;
        let submitted = submitted_lighting(&self.active, &report)?;
        write_lighting_report(self.session.device(), &report)?;
        self.active = submitted;
        Ok(())
    }
    pub fn send_color(&self, rgb: [u8; 3]) -> Result<()> {
        if self.active.effect_id() != 21 {
            return Err("Screen frame requires screen mode".into());
        }
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&byakko_protocol::nia87::host_lighting::screen_report(rgb));
        self.session.device().send_setter(&host)?;
        Ok(())
    }

    pub fn send_music(&self, bands: [u8; 32]) -> Result<()> {
        if !matches!(self.active.effect_id(), 20 | 22) {
            return Err("Music frame requires music mode".into());
        }
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&byakko_protocol::nia87::host_lighting::music_report(bands));
        self.session.device().send_setter(&host)?;
        Ok(())
    }

    fn restore(&mut self) -> ApplyResult<byakko_protocol::nia87::lighting::Lighting> {
        let setting = self
            .saved
            .recognized_setting()
            .ok_or_else(|| not_attempted("Unrecognized saved lighting"))?;
        let restored = apply_lighting_on_device(
            self.session.device(),
            &self.active,
            &setting,
            &self.backups,
            CompletionPolicy::VerifiedReadback,
        )?;
        self.finished = true;
        Ok(restored)
    }

    pub fn finish(mut self) -> ApplyResult<byakko_protocol::nia87::lighting::Lighting> {
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
) -> Result<byakko_protocol::nia87::lighting::Lighting> {
    let response = read_payload(
        device,
        byakko_protocol::nia87::lighting::LED_READ_COMMAND,
        0,
        0,
    )?;
    if response[0] != byakko_protocol::nia87::lighting::LED_READ_COMMAND {
        return Err("Lighting read returned an unrelated opcode".into());
    }
    Ok(byakko_protocol::nia87::lighting::Lighting::decode(
        &response,
    )?)
}

pub(super) fn write_lighting_report(device: &HidDevice, report: &[u8; 64]) -> Result<()> {
    let mut host = [0u8; 65];
    host[1..].copy_from_slice(report);
    device.send_setter(&host)?;
    std::thread::sleep(pacing::LIGHTING_SETTER);
    Ok(())
}

/// Submit ordinary global lighting with a durable raw backup. A successful
/// return means the transport accepted the setter, not that a getter verified
/// firmware persistence. Host start and restore use the verified path below.
pub fn apply_lighting(
    expected: &byakko_protocol::nia87::lighting::Lighting,
    setting: &byakko_protocol::nia87::lighting::LightingSetting,
    backup_dir: &std::path::Path,
) -> ApplyResult<byakko_protocol::nia87::lighting::Lighting> {
    apply_lighting_with(Selection::Unique, expected, setting, backup_dir)
}

pub(super) fn apply_lighting_with(
    selection: Selection<'_>,
    expected: &byakko_protocol::nia87::lighting::Lighting,
    setting: &byakko_protocol::nia87::lighting::LightingSetting,
    backup_dir: &std::path::Path,
) -> ApplyResult<byakko_protocol::nia87::lighting::Lighting> {
    let session = Session::open_for(selection).map_err(not_attempted)?;
    apply_lighting_on_device(
        session.device(),
        expected,
        setting,
        backup_dir,
        CompletionPolicy::TransportAccepted,
    )
}

fn apply_lighting_on_device(
    device: &HidDevice,
    expected: &byakko_protocol::nia87::lighting::Lighting,
    setting: &byakko_protocol::nia87::lighting::LightingSetting,
    backup_dir: &std::path::Path,
    policy: CompletionPolicy,
) -> ApplyResult<byakko_protocol::nia87::lighting::Lighting> {
    if expected.raw()[0] != byakko_protocol::nia87::lighting::LED_READ_COMMAND
        || expected.recognized_setting().is_none()
    {
        return Err(not_attempted(
            "Lighting baseline is not a recognized Nia87 LED response",
        ));
    }
    let target = byakko_protocol::nia87::lighting::write_report(setting).map_err(not_attempted)?;
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
    )
    .map_err(not_attempted)?;

    if matches!(policy, CompletionPolicy::TransportAccepted) {
        // The captured official UI updates its cache after the setter without
        // a getter. Keep transport acceptance distinct from verified reads.
        let submitted = submitted_lighting(expected, &target).map_err(not_attempted)?;
        submit_lighting_report(&target, backup.path(), |report| {
            write_lighting_report(device, report)
        })?;
        return Ok(submitted);
    }

    let restore_report = lighting_restore_report(expected);
    apply_roundtrip(
        &backup,
        VerifiedStep {
            write: || write_lighting_report(device, &target),
            matches: |actual: &byakko_protocol::nia87::lighting::Lighting| {
                lighting_matches_report(actual, &target, expected)
            },
            mismatch: "Lighting readback differs in setting or reserved response bytes",
        },
        VerifiedStep {
            write: || write_lighting_report(device, &restore_report),
            matches: |actual: &byakko_protocol::nia87::lighting::Lighting| {
                lighting_matches_report(actual, &restore_report, expected)
            },
            mismatch: "Lighting restoration could not be verified",
        },
        || read_lighting_on_device(device),
        lighting_apply_error,
    )
}

fn submit_lighting_report(
    report: &[u8; 64],
    backup: &std::path::Path,
    mut send: impl FnMut(&[u8; 64]) -> Result<()>,
) -> ApplyResult<()> {
    send(report).map_err(|error| {
        ApplyFailure {
            message: format!(
                "Lighting upload stopped after a transport error: {error}. Device state is unknown; no automatic restore sent. Backup: {}",
                backup.display()
            ),
            recovery: Recovery::Unverified,
        }
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

        let failure = submit_lighting_report(&report, backup, |_| {
            sends += 1;
            Err("Disconnected".into())
        })
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
        let expected = byakko_protocol::nia87::lighting::Lighting::decode(&original).unwrap();
        let mut report = [0; 64];
        report[..8].copy_from_slice(&[0x07, 2, 3, 2, 8, 9, 8, 7]);
        let submitted = submitted_lighting(&expected, &report).unwrap();
        assert_eq!(submitted.raw()[0], 0x87);
        assert_eq!(&submitted.raw()[1..8], &report[1..8]);
        assert_eq!(&submitted.raw()[8..], &original[8..]);
    }
}
