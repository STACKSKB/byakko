use crate::hid::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// OS-held lock survives neither crashes nor process exit. The empty lock file
// stays on disk; existence alone never means that a transaction is active.
fn transaction_lock() -> Result<std::fs::File> {
    let path = std::env::temp_dir().join("byakko-nia87-configuration.lock");
    lock_file(&path)
}

fn lock_file(path: &std::path::Path) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .map_err(|_| "Another Byakko transaction is active; retry after it finishes")?;
    Ok(file)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

pub fn candidates() -> Result<Vec<Candidate>> {
    let api = HidApi::new()?;
    Ok(api
        .device_list()
        .filter(|d| {
            d.vendor_id() == 0x3151
                && matches!(d.product_id(), 0x4011 | 0x4015)
                && d.usage_page() == 0xffff
                && d.usage() == 2
        })
        .map(|d| Candidate {
            path: d.path().to_string_lossy().into_owned(),
            vid: d.vendor_id(),
            pid: d.product_id(),
            interface: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            manufacturer: d.manufacturer_string().map(str::to_owned),
            product: d.product_string().map(str::to_owned),
        })
        .collect())
}

pub fn open_unique() -> Result<(Candidate, HidDevice)> {
    let list = candidates()?;
    if list.len() != 1 {
        return Err(format!(
            "Expected one Nia87 configuration collection; found {}. Connect one keyboard by USB.",
            list.len()
        )
        .into());
    }
    let candidate = list.into_iter().next().unwrap();
    let api = HidApi::new()?;
    let path = std::ffi::CString::new(candidate.path.as_str())?;
    let device = api.open_path(&path)?;
    Ok((candidate, device))
}

pub fn descriptor() -> Result<serde_json::Value> {
    let (candidate, device) = open_unique()?;
    let mut bytes = [0u8; 4096];
    let len = device.get_report_descriptor(&mut bytes)?;
    Ok(
        serde_json::json!({"candidate":candidate,"report_descriptor_hex":bytes[..len].iter().map(|b|format!("{b:02x}")).collect::<Vec<_>>().join(" "),"length":len}),
    )
}

/// Two read-only requests verified against the Nia87-specific vendor call chain.
/// Sending a feature report selects the read operation; it does not mutate settings.
pub fn inspect() -> Result<serde_json::Value> {
    let _lock = transaction_lock()?;
    let (candidate, device) = open_unique()?;
    let mut replies = Vec::new();
    for opcode in [0x80u8, 0x85] {
        let mut request = [0u8; 65];
        request[1] = opcode;
        // BIT7 framing hypothesis: complement of bytes 0..6 at payload offset 7.
        // Restricted to known read-only opcodes until verified against the device.
        request[8] = !opcode;
        device.send_feature_report(&request)?;
        std::thread::sleep(std::time::Duration::from_millis(30));
        let mut reply = [0u8; 65];
        let n = device.get_feature_report(&mut reply)?;
        if n != 65 || reply[0] != 0 || reply[1] != opcode {
            return Err(format!(
                "Unexpected response to {opcode:02x}: length={n} bytes={:02x?}",
                &reply[..n]
            )
            .into());
        }
        replies.push(
            serde_json::json!({"opcode":opcode,"payload":&reply[1..],"request":&request[1..]}),
        );
    }
    Ok(serde_json::json!({"candidate":candidate,"replies":replies}))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub format_version: u32,
    pub firmware: u16,
    pub profile: u8,
    pub base: Vec<[u8; 4]>,
    pub function: Vec<[u8; 4]>,
}

// Production forwards directly. The explicitly enabled research build can
// simulate one transport failure while retaining a real device for recovery.
trait FeatureSetter {
    fn send_setter(&self, report: &[u8]) -> Result<()>;
}

impl FeatureSetter for HidDevice {
    fn send_setter(&self, report: &[u8]) -> Result<()> {
        #[cfg(feature = "research-tools")]
        let result = crate::research_fault::send(report, || self.send_feature_report(report));
        #[cfg(not(feature = "research-tools"))]
        let result = self.send_feature_report(report);
        // An error does not prove that the firmware did not receive the setter.
        // Callers normally sleep only after success; retain the longest known
        // setter interval before any recovery request on an uncertain result.
        if result.is_err() {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        result
    }
}

fn read_payload(device: &HidDevice, opcode: u8, index: u8, page: u8) -> Result<[u8; 64]> {
    let mut request = [0u8; 65];
    request[1..].copy_from_slice(&crate::protocol::read_request(opcode, index, page));
    device.send_feature_report(&request)?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    let mut reply = [0u8; 65];
    let n = device.get_feature_report(&mut reply)?;
    if n != 65 || reply[0] != 0 {
        return Err(format!("Invalid HID response length/prefix: {n}").into());
    }
    Ok(reply[1..].try_into()?)
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
    let _lock = transaction_lock()?;
    snapshot_unlocked()
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
    let _lock = transaction_lock()?;
    let (_, device) = open_unique()?;
    read_lighting_on_device(&device)
}

pub fn read_settings() -> Result<crate::settings::Settings> {
    let _lock = transaction_lock()?;
    let (_, device) = open_unique()?;
    read_settings_on_device(&device)
}

/// Exclusive host-lighting session. The lock covers setup, every frame, and
/// restoration. Explicit finish reports restoration errors to the caller.
pub struct HostLightingSession {
    _lock: std::fs::File,
    device: HidDevice,
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
        let lock = transaction_lock()?;
        let (_, device) = open_unique()?;
        if !read_settings_on_device(&device)?.backlight_enabled() {
            return Err("Enable the backlight in Settings before starting host lighting".into());
        }
        // apply_lighting performs identity and expected-state checks before mutation.
        let active = apply_lighting_unlocked(expected, desired, backups)?;
        Ok(Self {
            _lock: lock,
            device,
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
        self.device.send_setter(&host)?;
        Ok(())
    }

    pub fn send_music(&self, bands: [u8; 32]) -> Result<()> {
        if !matches!(self.active.effect_id(), 20 | 22) {
            return Err("Music frame requires music mode".into());
        }
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&crate::host_lighting::music_report(bands));
        self.device.send_setter(&host)?;
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
    use crate::settings::{Setting, Settings};
    let _lock = transaction_lock()?;
    let target_options = match setting {
        Setting::Backlight(enabled) => Some(expected.with_backlight(enabled)),
        _ => None,
    };
    let report = if let Some(ref target) = target_options {
        crate::settings::backlight_write_report(target.raw_reply(0x86).expect("options reply"))?
    } else {
        crate::settings::write_report(setting)?
    };
    let (_, device) = open_unique()?;
    let version = read_payload(&device, 0x80, 0, 0)?;
    let profile = read_payload(&device, 0x85, 0, 0)?;
    if version[0] != 0x80 || version[1..3] != [0, 1] || profile[0] != 0x85 || profile[1] != 0 {
        return Err("Unverified firmware/profile; no setting written".into());
    }
    if &read_settings_on_device(&device)? != expected {
        return Err("Settings changed since load; no write sent".into());
    }
    let mut replies: Vec<Vec<u8>> = [0x91, 0x97, 0x92, 0x86]
        .into_iter()
        .map(|opcode| expected.raw_reply(opcode).expect("known opcode").to_vec())
        .collect();
    let restore = match setting {
        Setting::Debounce(value) => {
            replies[0][2] = value;
            Setting::Debounce(expected.debounce())
        }
        Setting::AutoOs(value) => {
            replies[1][1] = u8::from(value);
            Setting::AutoOs(expected.auto_os())
        }
        Setting::Sleep(values) => {
            for (index, seconds) in values.into_iter().enumerate() {
                replies[2][1 + index * 2..3 + index * 2].copy_from_slice(&seconds.to_le_bytes());
            }
            Setting::Sleep(expected.sleep_seconds())
        }
        Setting::Backlight(_) => {
            replies[3] = target_options
                .as_ref()
                .expect("backlight target")
                .raw_reply(0x86)
                .expect("options reply")
                .to_vec();
            Setting::Backlight(expected.backlight_enabled())
        }
    };
    let restore_report = if matches!(setting, Setting::Backlight(_)) {
        crate::settings::backlight_write_report(expected.raw_reply(0x86).expect("options reply"))?
    } else {
        crate::settings::write_report(restore)?
    };
    let target = Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3])?;
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
    let _lock = transaction_lock()?;
    read_macro_unlocked(slot)
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
    let _lock = transaction_lock()?;
    read_picture_unlocked()
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

/// Capture all supported local configuration data without sending setters.
/// One handle and lock cover both complete sweeps. Other Byakko processes cannot
/// intervene; an external configurator must still be closed. Progress counts
/// completed macro slots across the two sweeps, out of 100.
pub fn capture_configuration(
    progress: impl FnMut(usize, usize),
) -> Result<crate::configuration::Configuration> {
    let _lock = transaction_lock()?;
    let (_, device) = open_unique()?;
    capture_configuration_on_device(&device, progress)
}

fn capture_configuration_on_device(
    device: &HidDevice,
    mut progress: impl FnMut(usize, usize),
) -> Result<crate::configuration::Configuration> {
    let mut capture = |pass: usize| -> Result<crate::configuration::Configuration> {
        let keymaps = snapshot_on_device(device)?;
        if keymaps.firmware != 0x0100 || keymaps.profile != 0 {
            return Err("Configuration capture requires firmware 0x0100, profile 0".into());
        }
        let lighting = read_lighting_on_device(device)?;
        let settings = read_settings_on_device(device)?;
        let picture = read_picture_on_device(device)?;
        let mut macros = Vec::with_capacity(50);
        for slot in 0..50 {
            macros.push(read_macro_on_device(device, slot)?);
            progress(pass * 50 + usize::from(slot) + 1, 100);
        }
        // Check identity and maps again after the longer macro sweep.
        if snapshot_on_device(device)? != keymaps {
            return Err("Keyboard identity or keymaps changed during configuration capture".into());
        }
        Ok(crate::configuration::Configuration {
            keymaps,
            macros,
            lighting,
            picture,
            settings,
        })
    };
    let first = capture(0)?;
    let second = capture(1)?;
    if first != second {
        return Err("Configuration changed between complete captures; no archive saved".into());
    }
    crate::configuration::validate(&first)?;
    Ok(first)
}

/// Apply a previously reviewed archive against an exact expected before-image.
/// The OS lock and HID handle remain owned through validation, backup, writes,
/// complete verification and any recovery attempt. Host capture is never started.
pub fn apply_configuration(
    expected: &crate::configuration::Configuration,
    target: &crate::configuration::Configuration,
    backup_dir: &std::path::Path,
    mut progress: impl FnMut(&str),
) -> Result<crate::configuration::Configuration> {
    let plan = crate::configuration_plan::plan(expected, target)?;
    // Recovery must be representable before the first setter is sent.
    let reverse = crate::configuration_plan::plan(target, expected)?;
    let _lock = transaction_lock()?;
    let (_, device) = open_unique()?;
    progress("Checking complete current configuration");
    if &capture_configuration_on_device(&device, |_, _| {})? != expected {
        return Err("Configuration changed since review; no writes sent".into());
    }
    if expected == target {
        return Ok(expected.clone());
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("configuration-before-{stamp}.json"));
    crate::configuration::save_new(&path, expected)?;
    let result = (|| -> Result<crate::configuration::Configuration> {
        progress("Writing reviewed configuration changes");
        write_configuration_changes(&device, expected, target, &plan)?;
        progress("Verifying complete configuration");
        let actual = capture_configuration_on_device(&device, |_, _| {})?;
        if &actual != target {
            return Err("Complete configuration readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            progress("Restoring original configuration after failure");
            let restore = recover_configuration(&device, target, expected, &reverse);
            Err(format!(
                "Configuration apply failed: {error}; restore: {}; backup {}",
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

fn recover_configuration(
    device: &HidDevice,
    attempted: &crate::configuration::Configuration,
    original: &crate::configuration::Configuration,
    reverse: &crate::configuration_plan::ChangeSummary,
) -> Result<()> {
    let version = read_payload(device, 0x80, 0, 0)?;
    let profile = read_payload(device, 0x85, 0, 0)?;
    if version[0..3] != [0x80, 0, 1] || profile[0..2] != [0x85, 0] {
        return Err("Recovery identity check failed; durable archive retained".into());
    }
    let mut failures = Vec::new();
    let mut attempt = |label: String, result: Result<()>| {
        if let Err(error) = result {
            failures.push(format!("{label}: {error}"));
        }
    };
    // Remove new bindings first, then restore their macro contents. Reread
    // between layers to detect a setter's unexpected effect on the other map.
    for function in [true, false] {
        let observed = match snapshot_on_device(device) {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                attempt(
                    format!("keymap read {function}; recovery limited to planned slots"),
                    Err(error),
                );
                None
            }
        };
        let wanted = if function {
            &original.keymaps.function
        } else {
            &original.keymaps.base
        };
        let attempted_map = if function {
            &attempted.keymaps.function
        } else {
            &attempted.keymaps.base
        };
        let observed_map = observed.as_ref().map(|snapshot| {
            if function {
                snapshot.function.as_slice()
            } else {
                snapshot.base.as_slice()
            }
        });
        let slots =
            match crate::recovery_keymaps::slots_to_restore(observed_map, attempted_map, wanted) {
                Ok(slots) => slots,
                Err(error) => {
                    attempt(
                        format!("keymap recovery plan {function}"),
                        Err(error.into()),
                    );
                    continue;
                }
            };
        for slot in slots {
            attempt(
                format!("key {function}/{slot}"),
                write_binding(device, function, 0, slot, wanted[slot]),
            );
        }
    }
    for &slot in &reverse.macro_slots {
        attempt(
            format!("macro {slot}"),
            write_macro_bytes(device, slot, &original.macros[usize::from(slot)]),
        );
    }
    for slot in 0..126 {
        if attempted.picture[slot] != original.picture[slot] {
            let result = (|| -> Result<()> {
                let report =
                    crate::lighting::per_key_color_report(0, slot as u8, original.picture[slot])?;
                let mut host = [0u8; 65];
                host[1..].copy_from_slice(&report);
                device.send_setter(&host)?;
                std::thread::sleep(std::time::Duration::from_millis(100));
                Ok(())
            })();
            attempt(format!("picture {slot}"), result);
        }
    }
    for &setting in &reverse.settings {
        let result = (|| -> Result<()> {
            let report = if matches!(setting, crate::settings::Setting::Backlight(_)) {
                crate::settings::backlight_write_report(
                    original
                        .settings
                        .raw_reply(0x86)
                        .expect("validated options"),
                )?
            } else {
                crate::settings::write_report(setting)?
            };
            let mut host = [0u8; 65];
            host[1..].copy_from_slice(&report);
            device.send_setter(&host)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            Ok(())
        })();
        attempt(format!("setting {setting:?}"), result);
    }
    if reverse.lighting {
        attempt(
            "lighting".into(),
            write_lighting_report(device, &lighting_restore_report(&original.lighting)),
        );
    }
    // A transient write error may still have delivered its packet. Only full
    // readback establishes recovery; do not skip it after an individual error.
    let first = capture_configuration_on_device(device, |_, _| {}).map_err(|e| e.to_string());
    let verified = crate::recovery_verification::verify(original, first, || {
        // An aborted Windows feature request can leave this handle unusable.
        // Reopen only for readback under the same lock, never to retry setters.
        let (_, fresh) = open_unique().map_err(|e| e.to_string())?;
        capture_configuration_on_device(&fresh, |_, _| {}).map_err(|e| e.to_string())
    });
    match verified {
        Ok(()) => Ok(()),
        Err(verification) => Err(format!(
            "Recovery unverified: {verification}; section failures: {}",
            failures.join("; ")
        )
        .into()),
    }
}

fn write_configuration_changes(
    device: &HidDevice,
    before: &crate::configuration::Configuration,
    target: &crate::configuration::Configuration,
    plan: &crate::configuration_plan::ChangeSummary,
) -> Result<()> {
    // Revalidate identity on this same handle before apply or recovery.
    let version = read_payload(device, 0x80, 0, 0)?;
    let profile = read_payload(device, 0x85, 0, 0)?;
    if version[0..3] != [0x80, 0, 1] || profile[0..2] != [0x85, 0] {
        return Err("Configuration write identity check failed".into());
    }
    // Macro contents precede the key bindings that refer to them.
    for &slot in &plan.macro_slots {
        write_macro_bytes(device, slot, &target.macros[usize::from(slot)])?;
        if read_macro_on_device(device, slot)? != target.macros[usize::from(slot)] {
            return Err(format!("Macro {slot} readback mismatch").into());
        }
    }
    for function in [false, true] {
        let prior = &before.keymaps;
        let (old, new) = if function {
            (&prior.function, &target.keymaps.function)
        } else {
            (&prior.base, &target.keymaps.base)
        };
        for slot in 0..126 {
            if old[slot] != new[slot] {
                write_binding(device, function, 0, slot, new[slot])?;
            }
        }
    }
    if snapshot_on_device(device)? != target.keymaps {
        return Err("Keymap section readback mismatch".into());
    }
    if plan.picture_keys > 0 {
        for slot in 0..126 {
            if before.picture[slot] != target.picture[slot] {
                let report =
                    crate::lighting::per_key_color_report(0, slot as u8, target.picture[slot])?;
                let mut host = [0u8; 65];
                host[1..].copy_from_slice(&report);
                device.send_setter(&host)?;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        if read_picture_on_device(device)? != target.picture {
            return Err("Picture section readback mismatch".into());
        }
    }
    for &setting in &plan.settings {
        let report = if matches!(setting, crate::settings::Setting::Backlight(_)) {
            crate::settings::backlight_write_report(
                target.settings.raw_reply(0x86).expect("validated options"),
            )?
        } else {
            crate::settings::write_report(setting)?
        };
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&report);
        device.send_setter(&host)?;
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !plan.settings.is_empty() && read_settings_on_device(device)? != target.settings {
        return Err("Settings section readback mismatch".into());
    }
    if plan.lighting {
        write_lighting_report(device, &lighting_restore_report(&target.lighting))?;
        if read_lighting_on_device(device)? != target.lighting {
            return Err("Lighting section readback mismatch".into());
        }
    }
    Ok(())
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
            Err(format!(
                "Apply failed: {error}. Restore result: {}. Backup: {}",
                match rollback {
                    Ok(()) => "original keymaps verified".to_owned(),
                    Err(e) => format!("FAILED: {e}"),
                },
                path.display()
            )
            .into())
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
    fn operating_system_lock_excludes_second_handle_and_releases_on_drop() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("byakko-lock-test-{}-{stamp}", std::process::id()));
        let first = super::lock_file(&path).unwrap();
        assert!(super::lock_file(&path).is_err());
        drop(first);
        assert!(super::lock_file(&path).is_ok());
        // Retain the empty test artifact in accordance with the no-deletion rule.
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
