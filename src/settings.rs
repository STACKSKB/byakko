//! Pure Nia87 scalar-settings codec. Reports are 64-byte device payloads,
//! excluding the host's leading HID report-ID byte.

use serde::{Deserialize, Deserializer, Serialize};

pub const REPORT_LEN: usize = 64;
pub const DEBOUNCE_READ: u8 = 0x91;
pub const AUTO_OS_READ: u8 = 0x97;
pub const SLEEP_READ: u8 = 0x92;
pub const OPTIONS_READ: u8 = 0x86;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Debounce(u8),
    AutoOs(bool),
    /// Bluetooth/2.4 GHz normal timers followed by their deep-sleep timers.
    Sleep([u16; 4]),
    /// Enabling also clears power-save, which otherwise suppresses lighting.
    Backlight(bool),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Settings {
    debounce_raw: Vec<u8>,
    auto_os_raw: Vec<u8>,
    sleep_raw: Vec<u8>,
    options_raw: Vec<u8>,
}

/// A complete, device-free decision for one guarded setting transaction.
pub(crate) struct SettingPlan {
    pub target: Settings,
    pub report: [u8; REPORT_LEN],
    pub restore_report: [u8; REPORT_LEN],
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            debounce_raw: Vec<u8>,
            auto_os_raw: Vec<u8>,
            sleep_raw: Vec<u8>,
            options_raw: Vec<u8>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::decode(
            &wire.debounce_raw,
            &wire.auto_os_raw,
            &wire.sleep_raw,
            &wire.options_raw,
        )
        .map_err(serde::de::Error::custom)
    }
}

fn check_reply(reply: &[u8], opcode: u8) -> Result<(), String> {
    if reply.len() != REPORT_LEN {
        return Err(format!(
            "settings reply 0x{opcode:02x} must be {REPORT_LEN} bytes, got {}",
            reply.len()
        ));
    }
    if reply[0] != opcode {
        return Err(format!(
            "settings reply opcode mismatch: expected 0x{opcode:02x}, got 0x{:02x}",
            reply[0]
        ));
    }
    Ok(())
}

impl Settings {
    /// Decode all four replies without changing any unknown or reserved byte.
    pub fn decode(
        debounce: &[u8],
        auto_os: &[u8],
        sleep: &[u8],
        options: &[u8],
    ) -> Result<Self, String> {
        for (reply, opcode) in [
            (debounce, DEBOUNCE_READ),
            (auto_os, AUTO_OS_READ),
            (sleep, SLEEP_READ),
            (options, OPTIONS_READ),
        ] {
            check_reply(reply, opcode)?;
        }
        Ok(Self {
            debounce_raw: debounce.to_vec(),
            auto_os_raw: auto_os.to_vec(),
            sleep_raw: sleep.to_vec(),
            options_raw: options.to_vec(),
        })
    }

    pub fn debounce(&self) -> u8 {
        self.debounce_raw[2]
    }

    pub fn auto_os(&self) -> bool {
        self.auto_os_raw[1] != 0
    }

    /// Bluetooth sleep, 2.4 GHz sleep, Bluetooth deep sleep, 2.4 GHz deep sleep.
    /// Values are raw seconds; zero means disabled.
    pub fn sleep_seconds(&self) -> [u16; 4] {
        let raw = &self.sleep_raw;
        [
            u16::from_le_bytes([raw[1], raw[2]]),
            u16::from_le_bytes([raw[3], raw[4]]),
            u16::from_le_bytes([raw[5], raw[6]]),
            u16::from_le_bytes([raw[7], raw[8]]),
        ]
    }

    pub fn option_profile(&self) -> u8 {
        self.options_raw[1]
    }

    pub fn option_flags(&self) -> u8 {
        self.options_raw[2]
    }

    pub fn fn_matrix_enabled(&self) -> bool {
        self.options_raw[3] & 1 != 0
    }

    pub fn power_save_value(&self) -> u8 {
        self.options_raw[4]
    }

    pub fn backlight_enabled(&self) -> bool {
        self.option_flags() & 0x10 == 0 && self.power_save_value() == 0
    }

    /// Prepare the expected reply while retaining every other option bit and byte.
    pub fn with_backlight(&self, enabled: bool) -> Self {
        let mut target = self.clone();
        if enabled {
            target.options_raw[2] &= !0x10;
            target.options_raw[4] = 0;
        } else {
            target.options_raw[2] |= 0x10;
        }
        target
    }

    /// Preserve raw replies while preparing both forward and recovery reports.
    /// No transport should run until both directions can be encoded.
    pub(crate) fn plan_change(&self, setting: Setting) -> Result<SettingPlan, String> {
        let mut target = match setting {
            Setting::Backlight(enabled) => self.with_backlight(enabled),
            _ => self.clone(),
        };
        let restore = match setting {
            Setting::Debounce(value) => {
                target.debounce_raw[2] = value;
                Setting::Debounce(self.debounce())
            }
            Setting::AutoOs(value) => {
                if self.auto_os_raw[1] > 1 {
                    return Err("auto-OS current value is not canonical 0 or 1 for rollback".into());
                }
                target.auto_os_raw[1] = u8::from(value);
                Setting::AutoOs(self.auto_os())
            }
            Setting::Sleep(values) => {
                for (bytes, value) in target.sleep_raw[1..9]
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(values)
                {
                    bytes.copy_from_slice(&value.to_le_bytes());
                }
                Setting::Sleep(self.sleep_seconds())
            }
            Setting::Backlight(_) => Setting::Backlight(self.backlight_enabled()),
        };
        let (report, restore_report) = match setting {
            Setting::Backlight(_) => (
                backlight_write_report(&target.options_raw)?,
                backlight_write_report(&self.options_raw)?,
            ),
            _ => (
                write_report(setting)?,
                write_report(restore)
                    .map_err(|error| format!("Cannot encode recovery setting: {error}"))?,
            ),
        };
        Ok(SettingPlan {
            target,
            report,
            restore_report,
        })
    }

    pub fn raw_reply(&self, opcode: u8) -> Option<&[u8]> {
        match opcode {
            DEBOUNCE_READ => Some(&self.debounce_raw),
            AUTO_OS_READ => Some(&self.auto_os_raw),
            SLEEP_READ => Some(&self.sleep_raw),
            OPTIONS_READ => Some(&self.options_raw),
            _ => None,
        }
    }
}

/// Four BIT7 read requests in the same order as `Settings::decode` arguments.
pub fn read_requests() -> [[u8; REPORT_LEN]; 4] {
    [DEBOUNCE_READ, AUTO_OS_READ, SLEEP_READ, OPTIONS_READ]
        .map(|opcode| crate::protocol::read_request(opcode, 0, 0))
}

/// Encode validated Nia87 scalar settings. Sleep writes use the current
/// setter's data offsets 8–15, independently verified by live restoration.
pub fn write_report(setting: Setting) -> Result<[u8; REPORT_LEN], String> {
    let mut report = [0u8; REPORT_LEN];
    match setting {
        Setting::Debounce(value) => {
            if !(1..=10).contains(&value) {
                return Err("Nia87 debounce must be between 1 and 10".into());
            }
            report[0] = 0x11;
            report[2] = value;
        }
        Setting::AutoOs(value) => {
            report[0] = 0x17;
            report[1] = u8::from(value);
        }
        Setting::Sleep(values) => {
            report[0] = 0x12;
            for (index, seconds) in values.into_iter().enumerate() {
                let minimum = if index < 2 { 60 } else { 600 };
                if seconds != 0 && (!(minimum..=3600).contains(&seconds) || seconds % 60 != 0) {
                    return Err("Sleep timers require whole minutes: normal 1–60, deep 10–60, or zero to disable".into());
                }
                report[8 + index * 2..10 + index * 2].copy_from_slice(&seconds.to_le_bytes());
            }
        }
        Setting::Backlight(_) => {
            return Err("backlight write requires the verified raw options reply".into());
        }
    }
    let sum = report[..7]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[7] = 0xffu8.wrapping_sub(sum);
    Ok(report)
}

/// Convert a verified raw options reply to the observed option-setter frame.
/// Unknown bytes are retained, including bytes beyond the setter header.
pub fn backlight_write_report(options_reply: &[u8]) -> Result<[u8; REPORT_LEN], String> {
    check_reply(options_reply, OPTIONS_READ)?;
    let mut report = [0u8; REPORT_LEN];
    report.copy_from_slice(options_reply);
    report[0] = 0x06;
    report[7] = !report[..7]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured() -> Settings {
        let mut debounce = [0u8; REPORT_LEN];
        debounce[..8].copy_from_slice(&[0x91, 0, 1, 0, 0, 0, 0, 0x6e]);
        let mut auto = [0u8; REPORT_LEN];
        auto[..8].copy_from_slice(&[0x97, 0, 0, 0, 0, 0, 0, 0x68]);
        let mut sleep = [0u8; REPORT_LEN];
        sleep[..9].copy_from_slice(&[0x92, 0x78, 0, 0x78, 0, 0x58, 2, 0x58, 2]);
        let mut options = [0u8; REPORT_LEN];
        options[..8].copy_from_slice(&[0x86, 0, 0x10, 0, 1, 0, 0, 0x79]);
        options[50] = 0xab;
        Settings::decode(&debounce, &auto, &sleep, &options).unwrap()
    }

    #[test]
    fn decodes_captured_fields_and_preserves_reserved_bytes() {
        let settings = captured();
        assert_eq!(settings.debounce(), 1);
        assert!(!settings.auto_os());
        assert_eq!(settings.sleep_seconds(), [120, 120, 600, 600]);
        assert_eq!(settings.option_flags(), 0x10);
        assert!(!settings.fn_matrix_enabled());
        assert_eq!(settings.power_save_value(), 1);
        assert_eq!(settings.raw_reply(OPTIONS_READ).unwrap()[50], 0xab);
        let restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn setting_plans_preserve_other_fields_and_prepare_recovery() {
        let before = captured();
        for (setting, opcode, offset, bytes, restore) in [
            (
                Setting::Debounce(4),
                DEBOUNCE_READ,
                2,
                vec![4],
                Setting::Debounce(1),
            ),
            (
                Setting::AutoOs(true),
                AUTO_OS_READ,
                1,
                vec![1],
                Setting::AutoOs(false),
            ),
            (
                Setting::Sleep([180, 0, 3600, 600]),
                SLEEP_READ,
                1,
                vec![180, 0, 0, 0, 16, 14, 88, 2],
                Setting::Sleep([120, 120, 600, 600]),
            ),
        ] {
            let plan = before.plan_change(setting).unwrap();
            for section in [DEBOUNCE_READ, AUTO_OS_READ, SLEEP_READ, OPTIONS_READ] {
                let mut expected = before.raw_reply(section).unwrap().to_vec();
                if section == opcode {
                    expected[offset..offset + bytes.len()].copy_from_slice(&bytes);
                }
                assert_eq!(plan.target.raw_reply(section).unwrap(), expected);
            }
            assert_eq!(plan.report, write_report(setting).unwrap());
            assert_eq!(plan.restore_report, write_report(restore).unwrap());
        }
        let on = before.plan_change(Setting::Backlight(true)).unwrap();
        assert_eq!(on.target, before.with_backlight(true));
        assert_eq!(
            on.report,
            backlight_write_report(&on.target.options_raw).unwrap()
        );
        // Recovery retains the original power-save byte, not just the enabled boolean.
        assert_eq!(
            on.restore_report,
            backlight_write_report(&before.options_raw).unwrap()
        );
        assert_eq!(on.restore_report[4], 1);
        assert_eq!(on.restore_report[50], 0xab);
        assert_eq!(before, captured());
    }

    #[test]
    fn setting_plan_rejects_unencodable_forward_or_recovery_values() {
        let before = captured();
        assert!(before.plan_change(Setting::Debounce(0)).is_err());
        assert!(before.plan_change(Setting::Sleep([1; 4])).is_err());
        let mut unrecognized = before.clone();
        unrecognized.debounce_raw[2] = 0;
        assert!(unrecognized.plan_change(Setting::Debounce(4)).is_err());
        assert_eq!(unrecognized.debounce(), 0);
        let mut unrecognized = before.clone();
        unrecognized.auto_os_raw[1] = 2;
        assert!(unrecognized.plan_change(Setting::AutoOs(false)).is_err());
        // An unknown, untouched field is retained when editing another setting.
        assert_eq!(
            unrecognized
                .plan_change(Setting::Debounce(4))
                .unwrap()
                .target
                .auto_os_raw[1],
            2
        );
        assert_eq!(before, captured());
    }

    #[test]
    fn rejects_truncated_or_wrong_opcode_replies() {
        let settings = captured();
        let d = settings.raw_reply(DEBOUNCE_READ).unwrap();
        let a = settings.raw_reply(AUTO_OS_READ).unwrap();
        let s = settings.raw_reply(SLEEP_READ).unwrap();
        let o = settings.raw_reply(OPTIONS_READ).unwrap();
        assert!(Settings::decode(&d[..63], a, s, o).is_err());
        assert!(Settings::decode(o, a, s, o).is_err());
    }

    #[test]
    fn requests_and_safe_setters_use_bit7_framing() {
        let requests = read_requests();
        assert_eq!(&requests[0][..8], &[0x91, 0, 0, 0, 0, 0, 0, 0x6e]);
        assert_eq!(&requests[1][..8], &[0x97, 0, 0, 0, 0, 0, 0, 0x68]);
        assert_eq!(
            &write_report(Setting::Debounce(4)).unwrap()[..8],
            &[0x11, 0, 4, 0, 0, 0, 0, 0xea]
        );
        assert_eq!(
            &write_report(Setting::AutoOs(true)).unwrap()[..8],
            &[0x17, 1, 0, 0, 0, 0, 0, 0xe7]
        );
        assert!(write_report(Setting::Debounce(0)).is_err());
        assert!(write_report(Setting::Debounce(11)).is_err());
    }

    #[test]
    fn sleep_uses_verified_current_layout_and_validates_all_fields() {
        let report = write_report(Setting::Sleep([180, 120, 600, 600])).unwrap();
        assert_eq!(&report[..8], &[0x12, 0, 0, 0, 0, 0, 0, 0xed]);
        assert_eq!(&report[8..16], &[180, 0, 120, 0, 88, 2, 88, 2]);
        assert!(write_report(Setting::Sleep([0; 4])).is_ok());
        for values in [
            [59, 120, 600, 600],
            [120, 3601, 600, 600],
            [120, 120, 60, 600],
            [120, 120, 600, 601],
        ] {
            assert!(write_report(Setting::Sleep(values)).is_err());
        }
    }

    #[test]
    fn backlight_setter_preserves_unknown_options_and_restores_exact_raw_state() {
        let mut before = captured();
        before.options_raw[2] = 0xb5; // Other option bits must survive either direction.
        before.options_raw[3] = 0x83;
        before.options_raw[4] = 7;
        before.options_raw[5] = 0x5a;
        let on = before.with_backlight(true);
        assert_eq!(on.option_flags(), 0xa5);
        assert_eq!(on.power_save_value(), 0);
        assert_eq!(on.raw_reply(OPTIONS_READ).unwrap()[50], 0xab);
        let write = backlight_write_report(on.raw_reply(OPTIONS_READ).unwrap()).unwrap();
        assert_eq!(&write[..8], &[6, 0, 0xa5, 0x83, 0, 0x5a, 0, 0x77]);
        assert_eq!(write[50], 0xab);
        assert_eq!(
            write[..7]
                .iter()
                .fold(0u8, |s, b| s.wrapping_add(*b))
                .wrapping_add(write[7]),
            0xff
        );
        let off = on.with_backlight(false);
        assert_eq!(off.option_flags(), 0xb5);
        assert_eq!(off.power_save_value(), 0);
        let restore = backlight_write_report(before.raw_reply(OPTIONS_READ).unwrap()).unwrap();
        assert_eq!(restore[2], 0xb5);
        assert_eq!(restore[4], 7);
        assert_eq!(restore[50], 0xab);
        assert!(write_report(Setting::Backlight(true)).is_err());
    }
}
