//! Portable, local Nia87 keymap files. Import only constructs a snapshot;
//! device writes remain an explicit operation in the caller.

use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::nia87::{device::Snapshot, keymap_policy};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const BOARD_ID: &str = "nia87";
const FILE_VERSION: u32 = 1;
const FIRMWARE: u16 = 0x0100;
const PROFILE: u8 = 0;
const MATRIX_LEN: usize = 128;
const WRITABLE_LEN: usize = 126;
const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    format: String,
    version: u32,
    board_id: String,
    firmware: u16,
    profile: u8,
    base: Vec<[u8; 4]>,
    function: Vec<[u8; 4]>,
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<()> {
    if snapshot.format_version != 1 {
        return Err("Unsupported device snapshot format".into());
    }
    if snapshot.firmware != FIRMWARE || snapshot.profile != PROFILE {
        return Err("Only Nia87 firmware 0x0100, profile 0 is supported".into());
    }
    if snapshot.base.len() != MATRIX_LEN || snapshot.function.len() != MATRIX_LEN {
        return Err("Both key matrices must contain exactly 128 four-byte bindings".into());
    }
    Ok(())
}

/// Encode the entire raw binding matrix, including unknown binding bytes and
/// the two reserved trailing slots. No interpretation or normalization occurs.
pub fn encode(snapshot: &Snapshot) -> Result<Vec<u8>> {
    validate_snapshot(snapshot)?;
    Ok(serde_json::to_vec_pretty(&ProfileFile {
        format: "byakko-keymap".into(),
        version: FILE_VERSION,
        board_id: BOARD_ID.into(),
        firmware: snapshot.firmware,
        profile: snapshot.profile,
        base: snapshot.base.clone(),
        function: snapshot.function.clone(),
    })?)
}

/// Parse a Byakko profile without contacting or modifying a keyboard.
pub fn decode(bytes: &[u8]) -> Result<Snapshot> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("Profile file is too large".into());
    }
    let file: ProfileFile = serde_json::from_slice(bytes)?;
    if file.format != "byakko-keymap" || file.version != FILE_VERSION {
        return Err("Unsupported local profile format or version".into());
    }
    if file.board_id != BOARD_ID {
        return Err("Profile is for a different keyboard".into());
    }
    let snapshot = Snapshot {
        format_version: 1,
        firmware: file.firmware,
        profile: file.profile,
        base: file.base,
        function: file.function,
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

/// Check fields that can only be judged against the currently connected
/// board. Slots 126 and 127 are saved verbatim but cannot be written.
pub fn validate_for_device(imported: &Snapshot, current: &Snapshot) -> Result<()> {
    validate_snapshot(imported)?;
    validate_snapshot(current)?;
    if imported.firmware != current.firmware || imported.profile != current.profile {
        return Err(
            "Profile firmware or device profile differs from the connected keyboard".into(),
        );
    }
    if imported.base[WRITABLE_LEN..] != current.base[WRITABLE_LEN..]
        || imported.function[WRITABLE_LEN..] != current.function[WRITABLE_LEN..]
    {
        return Err("Reserved keymap slots differ from the connected keyboard".into());
    }
    keymap_policy::validate_changes(
        &current.base,
        &current.function,
        &imported.base,
        &imported.function,
    )?;
    Ok(())
}

/// Create a new file exclusively and flush it before reporting success.
pub fn save_new(path: &Path, snapshot: &Snapshot) -> Result<()> {
    let bytes = encode(snapshot)?;
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    Ok(())
}

/// Import from a caller-selected path. This performs no device I/O.
pub fn load(path: &Path) -> Result<Snapshot> {
    let input = File::open(path)?;
    let mut bytes = Vec::new();
    input.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    decode(&bytes)
}

/// Import and reject reserved-slot differences before staging for this board.
pub fn load_for_device(path: &Path, current: &Snapshot) -> Result<Snapshot> {
    let imported = load(path)?;
    validate_for_device(&imported, current)?;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        let mut base = vec![[0; 4]; MATRIX_LEN];
        base[42] = [0x92, 0xff, 0x40, 0xa5];
        base[127] = [1, 2, 3, 4];
        Snapshot {
            format_version: 1,
            firmware: FIRMWARE,
            profile: PROFILE,
            base,
            function: vec![[0x72, 0x91, 0x19, 0x28]; MATRIX_LEN],
        }
    }

    #[test]
    fn round_trip_preserves_unknown_and_reserved_bytes() {
        let original = snapshot();
        let imported = decode(&encode(&original).unwrap()).unwrap();
        assert_eq!(imported, original);
        validate_for_device(&imported, &original).unwrap();
    }

    #[test]
    fn rejects_wrong_board_and_version() {
        let bytes = encode(&snapshot()).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["board_id"] = "other".into();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["board_id"] = BOARD_ID.into();
        value["version"] = 2.into();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn rejects_malformed_truncated_and_wrong_size() {
        let bytes = encode(&snapshot()).unwrap();
        assert!(decode(&bytes[..bytes.len() / 2]).is_err());
        assert!(decode(b"{garbage").is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["base"].as_array_mut().unwrap().pop();
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn rejects_reserved_slot_mismatch_and_unverified_firmware() {
        let current = snapshot();
        let mut imported = current.clone();
        imported.base[126][3] ^= 1;
        assert!(validate_for_device(&imported, &current).is_err());
        imported = current;
        imported.firmware = 0x0101;
        assert!(encode(&imported).is_err());
    }

    #[test]
    fn imported_unmapped_slot_is_archival_but_cannot_be_staged_for_write() {
        let current = snapshot();
        let mut imported = current.clone();
        imported.function[6][0] ^= 1;
        assert_eq!(decode(&encode(&imported).unwrap()).unwrap(), imported);
        assert!(
            validate_for_device(&imported, &current)
                .unwrap_err()
                .to_string()
                .contains("unmapped function keymap slot 6")
        );
    }
}
