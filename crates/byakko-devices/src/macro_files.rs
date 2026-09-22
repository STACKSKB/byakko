//! Bounded portable macro documents and legacy Nia87 import.
use crate::nia87::{macro_adapter, macro_file as legacy, macros as native};
use byakko_core::macros::{Content, Document};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub const MAX_FILE_BYTES: usize = 64 * 1024;

fn validate(document: &Document) -> Result<(), String> {
    if document.format_version != 2
        || document.backend_id.is_empty()
        || document.source_slot.is_empty()
    {
        return Err("Unsupported macro document version or empty backend/slot".into());
    }
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<Document, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Macro JSON exceeds 64 KiB".into());
    }
    let version: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let document = match version
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
    {
        Some(1) => {
            let old = legacy::decode(bytes)?;
            let raw = native::encode(&old.macro_data)?;
            let source_slot = macro_adapter::slot_id(old.slot);
            let snapshot = macro_adapter::from_bytes(&source_slot, &raw)?;
            let Content::Editable(program) = snapshot.content else {
                return Err("Legacy macro cannot be converted into an editable program".into());
            };
            Document {
                format_version: 2,
                backend_id: macro_adapter::BACKEND_ID.into(),
                source_slot,
                name: old.name,
                binding: Some(
                    match old.play_mode {
                        0 => "counted",
                        1 => "toggle",
                        2 => "hold",
                        _ => return Err("Invalid legacy play mode".into()),
                    }
                    .into(),
                ),
                program,
            }
        }
        _ => serde_json::from_value(version).map_err(|error| error.to_string())?,
    };
    validate(&document)?;
    Ok(document)
}

pub fn encode(document: &Document) -> Result<Vec<u8>, String> {
    validate(document)?;
    let mut bytes = serde_json::to_vec_pretty(document).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Macro JSON exceeds 64 KiB".into());
    }
    Ok(bytes)
}

pub fn load(path: &Path) -> Result<Document, String> {
    if !std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Macro path is not a regular file".into());
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Macro path is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    decode(&bytes)
}

pub fn save_new(path: &Path, document: &Document) -> Result<(), String> {
    let bytes = encode(document)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::macros::{Action, Event, Program};

    fn sample() -> Document {
        Document {
            format_version: 2,
            backend_id: "nia87".into(),
            source_slot: "slot-00".into(),
            name: "Saved".into(),
            binding: Some("hold".into()),
            program: Program {
                repeat_count: 0,
                events: vec![Event {
                    action: Action::Backend {
                        backend_id: "nia87".into(),
                        id: "wheel-left".into(),
                        pressed: true,
                    },
                    delay_ms: 0,
                }],
            },
        }
    }

    #[test]
    fn roundtrip_and_rejections() {
        let document = sample();
        assert_eq!(decode(&encode(&document).unwrap()).unwrap(), document);
        assert!(decode(&vec![b' '; MAX_FILE_BYTES + 1]).is_err());
        assert!(decode(b"{").is_err());
        let mut value = serde_json::to_value(&document).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value.as_object_mut().unwrap().remove("unexpected");
        value["format_version"] = serde_json::json!(3);
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut invalid = document;
        invalid.backend_id.clear();
        assert!(encode(&invalid).is_err());
    }

    #[test]
    fn legacy_conversion_preserves_metadata_and_actions() {
        let old = legacy::MacroFile {
            format_version: 1,
            slot: 49,
            name: "Old".into(),
            play_mode: 2,
            macro_data: native::Macro {
                repeat_count: 0,
                events: vec![native::MacroEvent::MouseButton {
                    button: 245,
                    down: true,
                    delay_ms: 0,
                }],
            },
        };
        let converted = decode(&legacy::encode(&old).unwrap()).unwrap();
        assert_eq!(converted.source_slot, "slot-49");
        assert_eq!(converted.name, "Old");
        assert_eq!(converted.binding.as_deref(), Some("hold"));
        assert_eq!(
            converted.program.events[0].action,
            Action::Backend {
                backend_id: "nia87".into(),
                id: "wheel-left".into(),
                pressed: true
            }
        );
        assert_eq!(decode(&encode(&converted).unwrap()).unwrap(), converted);
    }

    #[test]
    fn files_are_bounded_and_existing_outputs_are_never_overwritten() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("byakko-macro-files-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("macro.json");
        let document = sample();
        save_new(&path, &document).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut changed = document.clone();
        changed.name = "Must not replace existing file".into();
        assert!(save_new(&path, &changed).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(load(&path).unwrap(), document);
        assert!(load(&directory).is_err());
        let oversize = directory.join("oversize.json");
        changed.name = "x".repeat(MAX_FILE_BYTES);
        assert!(save_new(&oversize, &changed).is_err());
        assert!(
            !oversize.exists(),
            "serialize and validate before creating any output"
        );
        // Retain test artifacts: the workspace's user forbids file deletion.
    }
}
