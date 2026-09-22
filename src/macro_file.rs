//! Bounded, versioned native macro JSON file format. No device I/O.
use crate::macros::{self, Macro};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub const MAX_FILE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroFile {
    pub format_version: u32,
    pub slot: u8,
    pub name: String,
    pub play_mode: u8,
    pub macro_data: Macro,
}

fn validate(file: &MacroFile) -> Result<(), String> {
    if file.format_version != 1 || file.slot > 49 || file.play_mode > 2 {
        return Err("Unsupported macro JSON version, slot, or play mode".into());
    }
    macros::encode(&file.macro_data)?;
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<MacroFile, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Macro JSON exceeds 64 KiB".into());
    }
    let file: MacroFile = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    validate(&file)?;
    Ok(file)
}

pub fn encode(file: &MacroFile) -> Result<Vec<u8>, String> {
    validate(file)?;
    let mut bytes = serde_json::to_vec_pretty(file).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Macro JSON exceeds 64 KiB".into());
    }
    Ok(bytes)
}

fn read_bounded(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Macro JSON exceeds 64 KiB".into());
    }
    Ok(bytes)
}

pub fn load(path: &Path) -> Result<MacroFile, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    decode(&read_bounded(file)?)
}

pub fn save_new(path: &Path, file: &MacroFile) -> Result<(), String> {
    let bytes = encode(file)?;
    let mut handle = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    handle
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    handle.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::MacroEvent;

    fn file() -> MacroFile {
        MacroFile {
            format_version: 1,
            slot: 49,
            name: "Example".into(),
            play_mode: 2,
            macro_data: Macro {
                repeat_count: 1,
                events: vec![
                    MacroEvent::Key {
                        usage: 4,
                        down: true,
                        delay_ms: 1
                    };
                    123
                ],
            },
        }
    }

    #[test]
    fn maximum_encoded_macro_round_trips() {
        let file = file();
        assert_eq!(macros::encode(&file.macro_data).unwrap()[247], 0x81);
        let bytes = encode(&file).unwrap();
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert_eq!(decode(&bytes).unwrap(), file);
    }

    #[test]
    fn rejects_malformed_truncated_and_unknown_fields() {
        assert!(decode(b"{").is_err());
        let bytes = encode(&file()).unwrap();
        assert!(decode(&bytes[..bytes.len() - 10]).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["unknown"] = serde_json::json!(true);
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn rejects_version_slot_mode_and_invalid_macro() {
        let mut value = file();
        value.format_version = 2;
        assert!(encode(&value).is_err());
        value.format_version = 1;
        value.slot = 50;
        assert!(encode(&value).is_err());
        value.slot = 0;
        value.play_mode = 3;
        assert!(encode(&value).is_err());
        value.play_mode = 0;
        value.macro_data.events[0] = MacroEvent::Key {
            usage: 3,
            down: true,
            delay_ms: 1,
        };
        assert!(encode(&value).is_err());
    }

    #[test]
    fn rejects_oversize_before_parse_and_bounds_reader() {
        let oversized = vec![b' '; MAX_FILE_BYTES + 1];
        assert!(decode(&oversized).unwrap_err().contains("64 KiB"));
        assert!(
            read_bounded(oversized.as_slice())
                .unwrap_err()
                .contains("64 KiB")
        );
        assert_eq!(
            read_bounded([b' '; MAX_FILE_BYTES].as_slice())
                .unwrap()
                .len(),
            MAX_FILE_BYTES
        );
    }
}
