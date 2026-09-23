use super::apply_error::lighting_apply_error;
use super::*;

/// Read global lighting state twice with an identity barrier before each read.
/// This only sends GET commands (0x80 and 0x87). A matching opcode echo and
/// identical replies are required before returning the raw-preserving decode.
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
    saved: crate::nia87::lighting::Lighting,
    active: crate::nia87::lighting::Lighting,
    backups: std::path::PathBuf,
    finished: bool,
}

pub type ScreenSession = HostLightingSession;

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
        if !matches!(desired.effect_id, 20..=22) {
            return Err("Host lighting requires screen or music mode".into());
        }
        let session = Session::open()?;
        if !read_settings_on_device(session.device())?.backlight_enabled() {
            return Err("Enable the backlight in Settings before starting host lighting".into());
        }
        // apply_lighting performs identity and expected-state checks before mutation.
        let active = apply_lighting_unlocked(Selection::Unique, expected, desired, backups)?;
        Ok(Self {
            session,
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
        // Expected-state checks prevent overwriting settings changed externally.
        let restored =
            apply_lighting_unlocked(Selection::Unique, &self.active, &setting, &self.backups)?;
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
    let mut first = None;
    for _ in 0..2 {
        let barrier = read_payload(device, 0x80, 0, 0)?;
        if barrier[0] != 0x80 {
            return Err("Lighting read identity barrier failed; close other configurators".into());
        }
        let response = read_payload(device, crate::nia87::lighting::LED_READ_COMMAND, 0, 0)?;
        if response[0] != crate::nia87::lighting::LED_READ_COMMAND {
            return Err("Lighting read returned an unrelated or stale opcode".into());
        }
        if let Some(previous) = first {
            if previous != response {
                return Err("Lighting changed between repeated reads".into());
            }
        } else {
            first = Some(response);
        }
    }
    Ok(crate::nia87::lighting::Lighting::decode(
        &first.expect("two reads were requested"),
    )?)
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
    std::thread::sleep(std::time::Duration::from_millis(500));
    Ok(())
}

pub(super) fn lighting_matches_report(
    actual: &crate::nia87::lighting::Lighting,
    report: &[u8; 64],
    original: &crate::nia87::lighting::Lighting,
) -> bool {
    actual.raw()[1..8] == report[1..8] && actual.raw()[9..] == original.raw()[9..]
}

/// Stage-safe global lighting apply with a durable raw backup and restoration
/// attempt. The target and restore use the statically traced PB BIT8 format;
/// callers should treat its checksum as unverified until a live readback does.
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
    apply_lighting_unlocked(selection, expected, setting, backup_dir)
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
) -> Result<crate::nia87::lighting::Lighting> {
    if expected.raw()[0] != crate::nia87::lighting::LED_READ_COMMAND
        || expected.recognized_setting().is_none()
    {
        return Err("Lighting baseline is not a recognized Nia87 LED response".into());
    }
    let target = crate::nia87::lighting::write_report(setting)?;
    let (_, device) = selection.open()?;
    let version = read_payload(&device, 0x80, 0, 0)?;
    let profile = read_payload(&device, 0x85, 0, 0)?;
    if version[0] != 0x80 || profile[0] != 0x85 {
        return Err("Identity response mismatch; no lighting write sent".into());
    }
    if u16::from_le_bytes([version[1], version[2]]) != 0x0100 || profile[1] != 0 {
        return Err(
            "Firmware/profile differs from validated Nia87 0x0100/profile 0; no write sent".into(),
        );
    }
    let current = read_lighting_on_device(&device)?;
    if &current != expected {
        return Err("Lighting changed since load; reload before applying. No write sent".into());
    }
    if lighting_matches_report(&current, &target, expected) {
        return Ok(current);
    }

    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("lighting-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(
        &mut backup,
        &serde_json::json!({
            "format_version": 1,
            "firmware": 0x0100,
            "profile": 0,
            "before": current,
            "target_report": target.as_slice(),
        }),
    )?;
    backup.sync_all()?;

    let result = (|| -> Result<crate::nia87::lighting::Lighting> {
        write_lighting_report(&device, &target)?;
        let actual = read_lighting_on_device(&device)?;
        if !lighting_matches_report(&actual, &target, expected) {
            return Err("Lighting readback differs in setting or reserved response bytes".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let restore = (|| -> Result<()> {
                let report = lighting_restore_report(expected);
                write_lighting_report(&device, &report)?;
                let actual = read_lighting_on_device(&device)?;
                if !lighting_matches_report(&actual, &report, expected) {
                    return Err("Lighting restoration could not be verified".into());
                }
                Ok(())
            })();
            Err(lighting_apply_error(error.as_ref(), restore, &path).into())
        }
    }
}
