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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Settings {
    debounce_raw: Vec<u8>,
    auto_os_raw: Vec<u8>,
    sleep_raw: Vec<u8>,
    options_raw: Vec<u8>,
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

/// Encode only the two settings with an unambiguous Nia87 setter layout.
/// Sleep writes remain disabled because their byte-7 data overlaps generic
/// BIT7 checksum framing.
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
    }
    let sum = report[..7]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[7] = 0xffu8.wrapping_sub(sum);
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
}
