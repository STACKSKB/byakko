mod configuration;
mod transport;

use crate::hid::HidDevice;
use serde::{Deserialize, Serialize};

pub use configuration::{apply_configuration, capture_configuration};
pub use transport::{Candidate, candidates, descriptor, inspect, open_unique};
use transport::{FeatureSetter, Session, read_payload, transaction_lock};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub format_version: u32,
    pub firmware: u16,
    pub profile: u8,
    pub base: Vec<[u8; 4]>,
    pub function: Vec<[u8; 4]>,
}

fn read_matrix(device: &HidDevice, opcode: u8, index: u8) -> Result<Vec<[u8; 4]>> {
    let mut bytes = Vec::with_capacity(512);
    for page in 0..8 {
        // Interleave a verified scalar read to detect unchanged stale responses.
        let barrier = read_payload(device, 0x80, 0, 0)?;
        if barrier[0] != 0x80 {
            return Err(
                "Version barrier returned unrelated data; close other configurators".into(),
            );
        }
        let data = read_payload(device, opcode, index, page)?;
        if data == barrier {
            return Err(format!("Stale response to matrix page {page}").into());
        }
        bytes.extend_from_slice(&data);
    }
    Ok(bytes.as_chunks::<4>().0.to_vec())
}

pub fn snapshot() -> Result<Snapshot> {
    let session = Session::open()?;
    snapshot_on_device(session.device())
}

fn snapshot_unlocked() -> Result<Snapshot> {
    let (_, device) = open_unique()?;
    snapshot_on_device(&device)
}

fn snapshot_on_device(device: &HidDevice) -> Result<Snapshot> {
    let v = read_payload(device, 0x80, 0, 0)?;
    let p = read_payload(device, 0x85, 0, 0)?;
    if v[0] != 0x80 || p[0] != 0x85 {
        return Err("Unrelated identity response; close other configurators".into());
    }
    let base = read_matrix(device, 0x89, p[1])?;
    let function = read_matrix(device, 0x90, 0)?;
    if base != read_matrix(device, 0x89, p[1])? || function != read_matrix(device, 0x90, 0)? {
        return Err("Keymap changed between repeated reads; backup not trusted".into());
    }
    Ok(Snapshot {
        format_version: 1,
        firmware: u16::from_le_bytes([v[1], v[2]]),
        profile: p[1],
        base,
        function,
    })
}

/// Read global lighting state twice with an identity barrier before each read.
/// This only sends GET commands (0x80 and 0x87). A matching opcode echo and
/// identical replies are required before returning the raw-preserving decode.
pub fn read_lighting() -> Result<crate::lighting::Lighting> {
    let session = Session::open()?;
    read_lighting_on_device(session.device())
}

pub fn read_settings() -> Result<crate::settings::Settings> {
    let session = Session::open()?;
    read_settings_on_device(session.device())
}

/// Exclusive host-lighting session. The lock covers setup, every frame, and
/// restoration. Explicit finish reports restoration errors to the caller.
pub struct HostLightingSession {
    session: Session,
    saved: crate::lighting::Lighting,
    active: crate::lighting::Lighting,
    backups: std::path::PathBuf,
    finished: bool,
}

pub type ScreenSession = HostLightingSession;

impl HostLightingSession {
    pub fn start(expected: &crate::lighting::Lighting, backups: &std::path::Path) -> Result<Self> {
        let desired = crate::lighting::LightingSetting {
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
        expected: &crate::lighting::Lighting,
        desired: &crate::lighting::LightingSetting,
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
        let active = apply_lighting_unlocked(expected, desired, backups)?;
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
        host[1..].copy_from_slice(&crate::host_lighting::screen_report(rgb));
        self.session.device().send_setter(&host)?;
        Ok(())
    }

    pub fn send_music(&self, bands: [u8; 32]) -> Result<()> {
        if !matches!(self.active.effect_id(), 20 | 22) {
            return Err("Music frame requires music mode".into());
        }
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&crate::host_lighting::music_report(bands));
        self.session.device().send_setter(&host)?;
        Ok(())
    }

    fn restore(&mut self) -> Result<crate::lighting::Lighting> {
        let setting = self
            .saved
            .recognized_setting()
            .ok_or("Unrecognized saved lighting")?;
        // Expected-state checks prevent overwriting settings changed externally.
        let restored = apply_lighting_unlocked(&self.active, &setting, &self.backups)?;
        self.finished = true;
        Ok(restored)
    }

    pub fn finish(mut self) -> Result<crate::lighting::Lighting> {
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

fn read_settings_on_device(device: &HidDevice) -> Result<crate::settings::Settings> {
    let mut previous = None;
    for _ in 0..2 {
        let mut replies = Vec::new();
        for opcode in [0x91, 0x97, 0x92, 0x86] {
            if read_payload(device, 0x80, 0, 0)?[0] != 0x80 {
                return Err("Settings identity barrier failed".into());
            }
            replies.push(read_payload(device, opcode, 0, 0)?);
        }
        let settings =
            crate::settings::Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3])?;
        if let Some(ref first) = previous
            && first != &settings
        {
            return Err("Settings changed between reads".into());
        }
        previous = Some(settings);
    }
    Ok(previous.expect("two reads"))
}

pub fn apply_setting(
    expected: &crate::settings::Settings,
    setting: crate::settings::Setting,
    backup_dir: &std::path::Path,
) -> Result<crate::settings::Settings> {
    use crate::settings::{SettingPlan, Settings};
    let _lock = transaction_lock()?;
    let SettingPlan {
        target,
        report,
        restore_report,
    } = expected.plan_change(setting)?;
    let (_, device) = open_unique()?;
    let version = read_payload(&device, 0x80, 0, 0)?;
    let profile = read_payload(&device, 0x85, 0, 0)?;
    if version[0] != 0x80 || version[1..3] != [0, 1] || profile[0] != 0x85 || profile[1] != 0 {
        return Err("Unverified firmware/profile; no setting written".into());
    }
    if &read_settings_on_device(&device)? != expected {
        return Err("Settings changed since load; no write sent".into());
    }
    if &target == expected {
        return Ok(target);
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("settings-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(
        &mut backup,
        &serde_json::json!({"format_version":1,"before":expected,"target":target}),
    )?;
    backup.sync_all()?;
    let send = |data: &[u8; 64]| -> Result<()> {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(data);
        device.send_setter(&host)?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        Ok(())
    };
    let result = (|| -> Result<Settings> {
        send(&report)?;
        let actual = read_settings_on_device(&device)?;
        if actual != target {
            return Err("Setting readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let restore = (|| -> Result<()> {
                send(&restore_report)?;
                if &read_settings_on_device(&device)? != expected {
                    return Err("Settings restoration mismatch".into());
                }
                Ok(())
            })();
            Err(format!(
                "Setting apply failed: {error}; restore: {}; backup {}",
                match restore {
                    Ok(()) => "verified".into(),
                    Err(e) => e.to_string(),
                },
                path.display()
            )
            .into())
        }
    }
}

fn read_lighting_on_device(device: &HidDevice) -> Result<crate::lighting::Lighting> {
    let mut first = None;
    for _ in 0..2 {
        let barrier = read_payload(device, 0x80, 0, 0)?;
        if barrier[0] != 0x80 {
            return Err("Lighting read identity barrier failed; close other configurators".into());
        }
        let response = read_payload(device, crate::lighting::LED_READ_COMMAND, 0, 0)?;
        if response[0] != crate::lighting::LED_READ_COMMAND {
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
    Ok(crate::lighting::Lighting::decode(
        &first.expect("two reads were requested"),
    )?)
}

/// Rebuild only the global setting bytes exposed by PB's LED writer. The
/// unknown response tail remains in the backup, but is not sent as an
/// undocumented command payload during restoration.
fn lighting_restore_report(original: &crate::lighting::Lighting) -> [u8; 64] {
    let mut report = [0u8; 64];
    report[0] = crate::lighting::LED_WRITE_COMMAND;
    report[1..8].copy_from_slice(&original.raw()[1..8]);
    let sum = report[..8]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[8] = 0xffu8.wrapping_sub(sum);
    report
}

fn write_lighting_report(device: &HidDevice, report: &[u8; 64]) -> Result<()> {
    let mut host = [0u8; 65];
    host[1..].copy_from_slice(report);
    device.send_setter(&host)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    Ok(())
}

fn lighting_matches_report(
    actual: &crate::lighting::Lighting,
    report: &[u8; 64],
    original: &crate::lighting::Lighting,
) -> bool {
    actual.raw()[1..8] == report[1..8] && actual.raw()[9..] == original.raw()[9..]
}

/// Stage-safe global lighting apply with a durable raw backup and restoration
/// attempt. The target and restore use the statically traced PB BIT8 format;
/// callers should treat its checksum as unverified until a live readback does.
pub fn apply_lighting(
    expected: &crate::lighting::Lighting,
    setting: &crate::lighting::LightingSetting,
    backup_dir: &std::path::Path,
) -> Result<crate::lighting::Lighting> {
    let _lock = transaction_lock()?;
    apply_lighting_unlocked(expected, setting, backup_dir)
}

fn apply_lighting_unlocked(
    expected: &crate::lighting::Lighting,
    setting: &crate::lighting::LightingSetting,
    backup_dir: &std::path::Path,
) -> Result<crate::lighting::Lighting> {
    if expected.raw()[0] != crate::lighting::LED_READ_COMMAND
        || expected.recognized_setting().is_none()
    {
        return Err("Lighting baseline is not a recognized Nia87 LED response".into());
    }
    let target = crate::lighting::write_report(setting)?;
    let (_, device) = open_unique()?;
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

    let result = (|| -> Result<crate::lighting::Lighting> {
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
            Err(format!(
                "Lighting apply failed: {error}. Restore result: {}. Backup: {}",
                match restore {
                    Ok(()) => "original setting and reserved response bytes verified".to_owned(),
                    Err(error) => format!("FAILED: {error}"),
                },
                path.display()
            )
            .into())
        }
    }
}

pub fn read_macro(slot: u8) -> Result<Vec<u8>> {
    let session = Session::open()?;
    read_macro_on_device(session.device(), slot)
}

fn read_macro_unlocked(slot: u8) -> Result<Vec<u8>> {
    let (_, device) = open_unique()?;
    read_macro_on_device(&device, slot)
}

fn read_macro_on_device(device: &HidDevice, slot: u8) -> Result<Vec<u8>> {
    stable_macro_reads(|| {
        let mut bytes = Vec::with_capacity(256);
        for page in 0..4 {
            crate::macros::read_request(slot, page)?;
            let barrier = read_payload(device, 0x80, 0, 0)?;
            if barrier[0] != 0x80 {
                return Err("Macro read identity barrier failed".into());
            }
            let data = read_payload(device, 0x8b, slot, page)?;
            if data == barrier {
                return Err("Macro read returned stale identity data".into());
            }
            bytes.extend_from_slice(&data);
        }
        Ok(bytes)
    })
}

fn stable_macro_reads(mut read: impl FnMut() -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    // Immediately after a write the first page can contain its new first
    // 32 bytes and old remaining bytes. Require two consecutive full matching
    // snapshots, allowing one initial transitional snapshot, never a majority.
    let mut previous = None;
    for _ in 0..3 {
        let bytes = read()?;
        if bytes.len() != 256 {
            return Err("Incomplete macro snapshot".into());
        }
        if previous.as_ref() == Some(&bytes) {
            return Ok(bytes);
        }
        previous = Some(bytes);
    }
    Err("Macro did not stabilize across three complete reads".into())
}

/// Read the current custom lighting picture as 128 matrix-indexed RGB values.
pub fn read_picture() -> Result<Vec<[u8; 3]>> {
    let session = Session::open()?;
    read_picture_on_device(session.device())
}

fn read_picture_unlocked() -> Result<Vec<[u8; 3]>> {
    let (_, device) = open_unique()?;
    read_picture_on_device(&device)
}

fn read_picture_on_device(device: &HidDevice) -> Result<Vec<[u8; 3]>> {
    stable_picture_reads(|| {
        let mut pages = Vec::new();
        for page in 0..6 {
            let barrier = read_payload(device, 0x80, 0, 0)?;
            if barrier[0] != 0x80 {
                return Err("Picture identity barrier failed".into());
            }
            let bytes = read_payload(device, 0x8c, 0, page)?;
            if bytes == barrier {
                return Err("Picture read returned stale identity".into());
            }
            pages.push(bytes);
        }
        Ok(crate::lighting::user_picture_from_pages(&pages)?)
    })
}

fn stable_picture_reads(mut read: impl FnMut() -> Result<Vec<[u8; 3]>>) -> Result<Vec<[u8; 3]>> {
    let mut previous = None;
    for _ in 0..3 {
        let colors = read()?;
        if colors.len() != 128 {
            return Err("Incomplete picture snapshot".into());
        }
        if previous.as_ref() == Some(&colors) {
            return Ok(colors);
        }
        previous = Some(colors);
    }
    Err("Picture did not stabilize across three complete reads".into())
}

/// Replace custom picture colors, preserving every unedited matrix slot.
pub fn apply_picture(
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> Result<Vec<[u8; 3]>> {
    let _lock = transaction_lock()?;
    if expected.len() != 128 || desired.len() != 128 || expected[126..] != desired[126..] {
        return Err("Invalid picture size or reserved-slot modification".into());
    }
    let identity = snapshot_unlocked()?;
    if identity.firmware != 0x0100 || identity.profile != 0 {
        return Err("Unverified firmware/profile; no picture writes sent".into());
    }
    if read_picture_unlocked()? != expected {
        return Err("Picture changed since load; no writes sent".into());
    }
    let changes: Vec<_> = (0..126).filter(|&i| expected[i] != desired[i]).collect();
    if changes.is_empty() {
        return Ok(expected.to_vec());
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("picture-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(
        &mut backup,
        &serde_json::json!({"format_version":1,"colors":expected}),
    )?;
    backup.sync_all()?;
    let (_, device) = open_unique()?;
    let write = |colors: &[[u8; 3]]| -> Result<()> {
        for &slot in &changes {
            let mut host = [0u8; 65];
            host[1..].copy_from_slice(&crate::lighting::per_key_color_report(
                0,
                slot as u8,
                colors[slot],
            )?);
            device.send_setter(&host)?;
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(())
    };
    let result = (|| -> Result<Vec<[u8; 3]>> {
        write(desired)?;
        let actual = read_picture_unlocked()?;
        if actual != desired {
            return Err("Picture readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let restore = (|| -> Result<()> {
                write(expected)?;
                if read_picture_unlocked()? != expected {
                    return Err("Picture restoration mismatch".into());
                }
                Ok(())
            })();
            Err(format!(
                "Picture apply failed: {error}; restore: {}; backup {}",
                match restore {
                    Ok(()) => "verified".into(),
                    Err(e) => e.to_string(),
                },
                path.display()
            )
            .into())
        }
    }
}

fn write_macro_bytes(device: &HidDevice, slot: u8, bytes: &[u8]) -> Result<()> {
    for report in crate::macros::write_reports(slot, bytes)? {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&report);
        device.send_setter(&host)?;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    Ok(())
}

pub fn apply_macro(
    slot: u8,
    expected: &[u8],
    new_macro: &crate::macros::Macro,
    backup_dir: &std::path::Path,
) -> Result<Vec<u8>> {
    let _lock = transaction_lock()?;
    let target = crate::macros::encode(new_macro)?;
    crate::macros::decode(expected)?; // Refuse to overwrite an unrecognized store we cannot restore.
    if read_macro_unlocked(slot)? != expected {
        return Err("Macro changed since load; reload before applying".into());
    }
    if target == expected {
        return Ok(target);
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("macro-{slot}-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(
        &mut backup,
        &serde_json::json!({"slot":slot,"bytes":expected}),
    )?;
    backup.sync_all()?;
    let (_, device) = open_unique()?;
    // The write handle may differ from the one used for the initial read.
    // Validate it before sending any macro reports, including restoration.
    let version = read_payload(&device, 0x80, 0, 0)?;
    let profile = read_payload(&device, 0x85, 0, 0)?;
    if version[0] != 0x80
        || u16::from_le_bytes([version[1], version[2]]) != 0x0100
        || profile[0] != 0x85
        || profile[1] != 0
    {
        return Err(
            "Write handle is not validated Nia87 firmware 0x0100/profile 0; no macro writes sent"
                .into(),
        );
    }
    let result = (|| -> Result<Vec<u8>> {
        write_macro_bytes(&device, slot, &target)?;
        let actual = read_macro_on_device(&device, slot)?;
        if actual != target {
            return Err("Macro readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let rollback = (|| -> Result<()> {
                write_macro_bytes(&device, slot, expected)?;
                if read_macro_on_device(&device, slot)? != expected {
                    return Err("macro restoration mismatch".into());
                }
                Ok(())
            })();
            Err(format!(
                "{error}; restore: {}; backup {}",
                match rollback {
                    Ok(()) => "verified".to_owned(),
                    Err(e) => e.to_string(),
                },
                path.display()
            )
            .into())
        }
    }
}

fn write_binding(
    device: &HidDevice,
    function: bool,
    index: u8,
    slot: usize,
    binding: [u8; 4],
) -> Result<()> {
    if slot >= 126 {
        return Err("The last two configuration slots are read-only padding".into());
    }
    let mut payload = [0u8; 65];
    payload[1..].copy_from_slice(&crate::protocol::single_key_report(
        function, index, slot, binding,
    )?);
    device.send_setter(&payload)?;
    // Firmware 0100 can cross-write base/Fn values when setters are only
    // 100 ms apart. A 1 s interval passed mixed-layer write/restore tests.
    std::thread::sleep(std::time::Duration::from_secs(1));
    Ok(())
}

#[derive(Debug)]
struct KeymapApplyError(byakko_core::session::ApplyFailure);

impl std::fmt::Display for KeymapApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.message)
    }
}

impl std::error::Error for KeymapApplyError {}

fn keymap_apply_error(
    error: &dyn std::fmt::Display,
    rollback: Result<()>,
    backup_path: &std::path::Path,
) -> KeymapApplyError {
    use byakko_core::session::{ApplyFailure, Recovery};
    let (recovery, restore_message) = match rollback {
        Ok(()) => (Recovery::Verified, "original keymaps verified".to_owned()),
        Err(error) => (Recovery::Failed, format!("FAILED: {error}")),
    };
    KeymapApplyError(ApplyFailure {
        message: format!(
            "Apply failed: {error}. Restore result: {restore_message}. Backup: {}",
            backup_path.display()
        ),
        recovery,
    })
}

/// The same guarded transaction as `apply_keymaps`, with a typed recovery
/// outcome for callers that must distinguish verified restore from failure.
pub fn apply_keymaps_detailed(
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> std::result::Result<Snapshot, byakko_core::session::ApplyFailure> {
    use byakko_core::session::{ApplyFailure, Recovery};
    apply_keymaps(expected, base, function, backup_dir).map_err(|error| {
        error
            .downcast_ref::<KeymapApplyError>()
            .map(|typed| typed.0.clone())
            .unwrap_or_else(|| ApplyFailure {
                message: error.to_string(),
                recovery: Recovery::NotAttempted,
            })
    })
}

pub fn apply_keymaps(
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> Result<Snapshot> {
    if expected.format_version != 1
        || expected.base.len() != 128
        || expected.function.len() != 128
        || base.len() != 128
        || function.len() != 128
    {
        return Err("Invalid keymap shape or backup format".into());
    }
    if expected.base[126..] != base[126..] || expected.function[126..] != function[126..] {
        return Err("Cannot modify reserved padding slots".into());
    }
    let _lock = transaction_lock()?;
    let current = snapshot_unlocked()?;
    if &current != expected {
        return Err(
            "Keyboard changed since it was loaded. Reload before applying; no writes sent.".into(),
        );
    }
    if current.firmware != 0x0100 || current.profile != 0 {
        return Err(
            "Firmware/profile differs from validated Nia87 0x0100/profile 0; no keymap writes sent"
                .into(),
        );
    }
    let changes: Vec<_> = (0..126)
        .flat_map(|slot| {
            [
                (false, slot, expected.base[slot], base[slot]),
                (true, slot, expected.function[slot], function[slot]),
            ]
        })
        .filter(|(_, _, old, new)| old != new)
        .collect();
    if changes.is_empty() {
        return Ok(current);
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("keymaps-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(&mut backup, &current)?;
    backup.sync_all()?;
    let (_, device) = open_unique()?;
    // The write handle is opened after the backup. Recheck its identity too,
    // so a reconnect cannot put these reports onto an unvalidated device.
    let version = read_payload(&device, 0x80, 0, 0)?;
    let profile = read_payload(&device, 0x85, 0, 0)?;
    if version[0] != 0x80
        || u16::from_le_bytes([version[1], version[2]]) != 0x0100
        || profile[0] != 0x85
        || profile[1] != 0
    {
        return Err(
            "Write handle is not validated Nia87 firmware 0x0100/profile 0; no writes sent".into(),
        );
    }
    let result = (|| -> Result<Snapshot> {
        // The official helper's captured final HID report for a Fn binding is
        // the single-key 0x15 command with index 0.
        for &(is_fn, slot, _, new) in &changes {
            if is_fn {
                continue;
            }
            write_binding(&device, false, current.profile, slot, new)?;
        }
        for &(is_fn, slot, _, new) in &changes {
            if is_fn {
                write_binding(&device, true, 0, slot, new)?;
            }
        }
        let actual = snapshot_unlocked()?;
        if actual.base != base
            || actual.function != function
            || actual.firmware != current.firmware
            || actual.profile != current.profile
        {
            let mismatch_path = backup_dir.join(format!("keymaps-mismatch-{stamp}.json"));
            let mut mismatch = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(mismatch_path)?;
            serde_json::to_writer_pretty(
                &mut mismatch,
                &serde_json::json!({"actual":actual,"desired_base":base,"desired_function":function}),
            )?;
            mismatch.sync_all()?;
            return Err("Readback does not match the complete intended keymaps".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let rollback = (|| -> Result<()> {
                // A failed setter may have changed either map. Read both maps
                // before recovery, restore Fn first, then reread both maps
                // because those Fn writes might also have affected base.
                for (is_fn, profile, attempted, original) in [
                    (true, 0, function, current.function.as_slice()),
                    (false, current.profile, base, current.base.as_slice()),
                ] {
                    let observed = snapshot_unlocked().ok();
                    let observed_map = observed.as_ref().map(|snapshot| {
                        if is_fn {
                            snapshot.function.as_slice()
                        } else {
                            snapshot.base.as_slice()
                        }
                    });
                    // Without a trustworthy read, restore only planned changes.
                    // Complete readback below must still prove restoration.
                    let slots = crate::recovery_keymaps::slots_to_restore(
                        observed_map,
                        attempted,
                        original,
                    )?;
                    for slot in slots {
                        write_binding(&device, is_fn, profile, slot, original[slot])?;
                    }
                }
                if snapshot_unlocked()? != current {
                    return Err("restored data could not be verified".into());
                }
                Ok(())
            })();
            Err(keymap_apply_error(error.as_ref(), rollback, &path).into())
        }
    }
}

#[cfg(test)]
mod lighting_tests {
    #[test]
    fn macro_stability_requires_consecutive_complete_copies() {
        let mut transitional = vec![0; 256];
        transitional[..32].fill(1);
        let complete = vec![1; 256];
        let mut samples = [transitional.clone(), complete.clone(), complete.clone()].into_iter();
        assert_eq!(
            super::stable_macro_reads(|| Ok(samples.next().unwrap())).unwrap(),
            complete
        );
        let mut alternating = [transitional.clone(), complete, transitional].into_iter();
        assert!(super::stable_macro_reads(|| Ok(alternating.next().unwrap())).is_err());
        assert!(super::stable_macro_reads(|| Ok(vec![0; 32])).is_err());
        assert!(super::stable_macro_reads(|| Err("Disconnected".into())).is_err());
    }

    #[test]
    fn reserved_slot_is_rejected_before_device_access() {
        let expected = super::Snapshot {
            format_version: 1,
            firmware: 0x100,
            profile: 0,
            base: vec![[0; 4]; 128],
            function: vec![[0; 4]; 128],
        };
        let mut base = expected.base.clone();
        let function = expected.function.clone();
        base[127] = [0, 0, 0x72, 0];
        let error = super::apply_keymaps(
            &expected,
            &base,
            &function,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("reserved padding"));
    }

    #[test]
    fn detailed_keymap_preflight_errors_have_no_recovery_attempt() {
        use byakko_core::session::Recovery;
        let expected = super::Snapshot {
            format_version: 1,
            firmware: 0x0100,
            profile: 0,
            base: vec![[0; 4]; 128],
            function: vec![[0; 4]; 128],
        };
        let short = vec![[0; 4]; 127];
        let failure = super::apply_keymaps_detailed(
            &expected,
            &short,
            &expected.function,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert_eq!(failure.recovery, Recovery::NotAttempted);
        assert!(failure.message.contains("Invalid keymap shape"));
        let mut reserved = expected.base.clone();
        reserved[127] = [1; 4];
        let failure = super::apply_keymaps_detailed(
            &expected,
            &reserved,
            &expected.function,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert_eq!(failure.recovery, Recovery::NotAttempted);
        assert!(failure.message.contains("reserved padding"));
    }

    #[test]
    fn keymap_rollback_error_preserves_diagnostic_and_typed_outcome() {
        use byakko_core::session::Recovery;
        let backup = std::path::Path::new("backup.json");
        let verified = super::keymap_apply_error(&"write failed", Ok(()), backup);
        assert_eq!(verified.0.recovery, Recovery::Verified);
        assert_eq!(
            verified.to_string(),
            "Apply failed: write failed. Restore result: original keymaps verified. Backup: backup.json"
        );
        let failed =
            super::keymap_apply_error(&"readback mismatch", Err("restore failed".into()), backup);
        assert_eq!(failed.0.recovery, Recovery::Failed);
        assert_eq!(
            failed.to_string(),
            "Apply failed: readback mismatch. Restore result: FAILED: restore failed. Backup: backup.json"
        );
    }

    use super::*;

    #[test]
    fn picture_requires_two_consecutive_complete_matches() {
        let old = vec![[0; 3]; 128];
        let new = vec![[12, 34, 56]; 128];
        let mut transition = [old.clone(), new.clone(), new.clone()].into_iter();
        assert_eq!(
            stable_picture_reads(|| Ok(transition.next().unwrap())).unwrap(),
            new
        );
        let mut oscillating = [old.clone(), new, old].into_iter();
        assert!(stable_picture_reads(|| Ok(oscillating.next().unwrap())).is_err());
        assert!(stable_picture_reads(|| Ok(vec![[0; 3]; 127])).is_err());
    }

    #[test]
    fn restore_report_recreates_known_fields_without_sending_opaque_tail() {
        let mut response = [0u8; 64];
        response[0] = crate::lighting::LED_READ_COMMAND;
        response[1..8].copy_from_slice(&[5, 2, 4, 7, 12, 34, 56]);
        response[9] = 0xa5;
        let original = crate::lighting::Lighting::decode(&response).unwrap();
        let report = lighting_restore_report(&original);
        assert_eq!(&report[..9], &[7, 5, 2, 4, 7, 12, 34, 56, 0x80]);
        assert!(report[9..].iter().all(|byte| *byte == 0));
        assert!(lighting_matches_report(&original, &report, &original));
        let mut changed = response;
        changed[9] = 0;
        let changed = crate::lighting::Lighting::decode(&changed).unwrap();
        assert!(!lighting_matches_report(&changed, &report, &original));
    }
}
