//! Local, lossless Nia87 configuration archive. Import and export perform no
//! device I/O. The caller must explicitly decide what to do with imported data.
//!
//! Version 1 is JSON with `format`, `version`, `board_id`, `firmware`,
//! `profile`, `keymaps`, `macros`, `lighting`, `picture`, and `settings`.
//! `keymaps` uses the device `Snapshot` shape (two 128-entry arrays of four
//! bytes); `macros` contains all 50 slots as 256-byte arrays, including
//! unrecognized bytes; `lighting` is `{ "raw": [64 bytes] }`; `picture` is
//! 128 RGB triples; and `settings` contains four 64-byte raw replies.

use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{device::Snapshot, lighting::Lighting, settings::Settings};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const FORMAT: &str = "byakko-configuration";
const VERSION: u32 = 1;
const BOARD_ID: &str = "nia87";
const FIRMWARE: u16 = 0x0100;
const PROFILE: u8 = 0;
const MATRIX_LEN: usize = 128;
const MACRO_SLOTS: usize = 50;
const MACRO_LEN: usize = 256;
const PICTURE_LEN: usize = 128;
/// Bound allocation while allowing the complete pretty-printed raw archive.
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub keymaps: Snapshot,
    /// Slot index 0 through 49. Bytes are kept even when the macro codec does
    /// not recognize them, so an archive never normalizes a captured slot.
    pub macros: Vec<Vec<u8>>,
    pub lighting: Lighting,
    pub picture: Vec<[u8; 3]>,
    pub settings: Settings,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    format: String,
    version: u32,
    board_id: String,
    firmware: u16,
    profile: u8,
    keymaps: Snapshot,
    macros: Vec<Vec<u8>>,
    lighting: Lighting,
    picture: Vec<[u8; 3]>,
    settings: Settings,
}

/// Check archive identity and all fixed-size data before serialization or use.
pub fn validate(config: &Configuration) -> Result<()> {
    let map = &config.keymaps;
    if map.format_version != VERSION || map.firmware != FIRMWARE || map.profile != PROFILE {
        return Err("unsupported Nia87 snapshot identity".into());
    }
    if map.base.len() != MATRIX_LEN || map.function.len() != MATRIX_LEN {
        return Err("keymap matrices must each contain 128 bindings".into());
    }
    if config.macros.len() != MACRO_SLOTS
        || config.macros.iter().any(|slot| slot.len() != MACRO_LEN)
    {
        return Err("archive must contain 50 macro slots of 256 bytes each".into());
    }
    if config.lighting.raw().len() != crate::lighting::REPORT_LEN
        || config.lighting.raw()[0] != crate::lighting::LED_READ_COMMAND
    {
        return Err("lighting reply must contain 64 bytes and opcode 0x87".into());
    }
    if config.picture.len() != PICTURE_LEN {
        return Err("user picture must contain 128 RGB triples".into());
    }
    for opcode in [
        crate::settings::DEBOUNCE_READ,
        crate::settings::AUTO_OS_READ,
        crate::settings::SLEEP_READ,
        crate::settings::OPTIONS_READ,
    ] {
        let Some(reply) = config.settings.raw_reply(opcode) else {
            return Err("missing settings reply".into());
        };
        if reply.len() != crate::settings::REPORT_LEN || reply[0] != opcode {
            return Err("invalid settings reply length or opcode".into());
        }
    }
    Ok(())
}

/// Encode the full raw configuration as a versioned JSON archive.
pub fn encode(config: &Configuration) -> Result<Vec<u8>> {
    validate(config)?;
    let archive = Archive {
        format: FORMAT.into(),
        version: VERSION,
        board_id: BOARD_ID.into(),
        firmware: FIRMWARE,
        profile: PROFILE,
        keymaps: config.keymaps.clone(),
        macros: config.macros.clone(),
        lighting: config.lighting.clone(),
        picture: config.picture.clone(),
        settings: config.settings.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&archive)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("configuration archive is too large".into());
    }
    Ok(bytes)
}

/// Decode a bounded archive without touching a keyboard.
pub fn decode(bytes: &[u8]) -> Result<Configuration> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("configuration archive is too large".into());
    }
    let archive: Archive = serde_json::from_slice(bytes)?;
    if archive.format != FORMAT
        || archive.version != VERSION
        || archive.board_id != BOARD_ID
        || archive.firmware != FIRMWARE
        || archive.profile != PROFILE
    {
        return Err("unsupported configuration archive identity or version".into());
    }
    let config = Configuration {
        keymaps: archive.keymaps,
        macros: archive.macros,
        lighting: archive.lighting,
        picture: archive.picture,
        settings: archive.settings,
    };
    validate(&config)?;
    Ok(config)
}

/// Create a new archive exclusively and flush it before reporting success.
pub fn save_new(path: &Path, config: &Configuration) -> Result<()> {
    let bytes = encode(config)?;
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    Ok(())
}

/// Read no more than the maximum archive size plus one byte from a local file.
pub fn load(path: &Path) -> Result<Configuration> {
    let input = File::open(path)?;
    let mut bytes = Vec::new();
    input.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    decode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Configuration {
        let mut macros = vec![vec![0; MACRO_LEN]; MACRO_SLOTS];
        macros[49][255] = 0xa5;
        macros[0][2] = 0xfa; // Unknown macro data remains archival.
        let mut replies = [[0u8; 64]; 4];
        for (reply, opcode) in replies.iter_mut().zip([
            crate::settings::DEBOUNCE_READ,
            crate::settings::AUTO_OS_READ,
            crate::settings::SLEEP_READ,
            crate::settings::OPTIONS_READ,
        ]) {
            reply[0] = opcode;
            reply[63] = 0xa5;
        }
        Configuration {
            keymaps: Snapshot {
                format_version: VERSION,
                firmware: FIRMWARE,
                profile: PROFILE,
                base: vec![[0x91, 0x92, 0x93, 0x94]; MATRIX_LEN],
                function: vec![[0xff, 0x00, 0x42, 0x80]; MATRIX_LEN],
            },
            macros,
            lighting: {
                let mut raw = [0xa5; 64];
                raw[0] = crate::lighting::LED_READ_COMMAND;
                Lighting::decode(&raw).unwrap()
            },
            picture: vec![[1, 2, 255]; PICTURE_LEN],
            settings: Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3]).unwrap(),
        }
    }

    #[test]
    fn round_trip_preserves_every_raw_section() {
        let original = example();
        assert_eq!(decode(&encode(&original).unwrap()).unwrap(), original);
    }

    #[test]
    fn rejects_wrong_identity_truncation_and_oversize() {
        let bytes = encode(&example()).unwrap();
        assert!(decode(&bytes[..bytes.len() / 2]).is_err());
        assert!(decode(&vec![b' '; MAX_FILE_BYTES as usize + 1]).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["board_id"] = "other".into();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["board_id"] = BOARD_ID.into();
        value["version"] = 2.into();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["version"] = VERSION.into();
        value["keymaps"]["firmware"] = 0x0101.into();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn rejects_missing_or_wrong_length_sections_and_opcode() {
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode(&example()).unwrap()).unwrap();
        value["macros"].as_array_mut().unwrap().pop();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["macros"] = serde_json::to_value(example().macros).unwrap();
        value["macros"][0].as_array_mut().unwrap().pop();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["macros"] = serde_json::to_value(example().macros).unwrap();
        value["picture"].as_array_mut().unwrap().pop();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["picture"] = serde_json::to_value(example().picture).unwrap();
        value["settings"]["options_raw"][0] = 0.into();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["settings"] = serde_json::to_value(example().settings).unwrap();
        value["lighting"]["raw"][0] = 0.into();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn maximum_byte_values_fit_file_limit() {
        let mut config = example();
        for slot in &mut config.macros {
            slot.fill(255);
        }
        config.keymaps.base.fill([255; 4]);
        config.keymaps.function.fill([255; 4]);
        config.picture.fill([255; 3]);
        let bytes = encode(&config).unwrap();
        assert!(bytes.len() as u64 <= MAX_FILE_BYTES);
        assert_eq!(decode(&bytes).unwrap(), config);
    }
}
