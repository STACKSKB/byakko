//! Local names for Nia87 macro slots. These labels are not device state.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;
const BACKEND_ID: &str = "nia87";
const SLOT_COUNT: usize = 50;
const MAX_NAME_BYTES: usize = 256;
const MAX_FILE_BYTES: usize = 32 * 1024;
const COLLISION_ATTEMPTS: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub version: u32,
    pub backend_id: String,
    pub names: Vec<String>,
}

impl Labels {
    pub fn new(names: Vec<String>) -> Result<Self, String> {
        let labels = Self {
            version: VERSION,
            backend_id: BACKEND_ID.into(),
            names,
        };
        labels.validate()?;
        Ok(labels)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != VERSION || self.backend_id != BACKEND_ID {
            return Err("Unsupported macro label format or backend".into());
        }
        if self.names.len() != SLOT_COUNT {
            return Err(format!("Macro labels require exactly {SLOT_COUNT} names"));
        }
        if self.names.iter().any(|name| name.len() > MAX_NAME_BYTES) {
            return Err(format!("Macro label exceeds {MAX_NAME_BYTES} UTF-8 bytes"));
        }
        Ok(())
    }
}

fn encode(labels: &Labels) -> Result<Vec<u8>, String> {
    labels.validate()?;
    let mut bytes = serde_json::to_vec_pretty(labels).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Macro label file exceeds 32 KiB".into());
    }
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<Labels, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Macro label file exceeds 32 KiB".into());
    }
    let labels: Labels = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    labels.validate()?;
    Ok(labels)
}

fn sequence(name: &str) -> Result<Option<u64>, String> {
    let Some(digits) = name
        .strip_prefix("label-")
        .and_then(|value| value.strip_suffix(".json"))
    else {
        return Ok(None);
    };
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    digits
        .parse::<u64>()
        .map(Some)
        .map_err(|_| "Macro label sequence exceeds u64 range".into())
}

fn highest(dir: &Path) -> Result<Option<(u64, PathBuf)>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut latest: Option<(u64, PathBuf)> = None;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(number) = sequence(name)? else {
            continue;
        };
        if latest.as_ref().is_none_or(|(current, _)| number > *current) {
            latest = Some((number, entry.path()));
        }
    }
    Ok(latest)
}

pub fn load_latest(dir: &Path) -> Result<Option<Labels>, String> {
    let Some((_, path)) = highest(dir)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    decode(&bytes).map(Some)
}

pub fn save_new(dir: &Path, labels: &Labels) -> Result<PathBuf, String> {
    let bytes = encode(labels)?; // Validate before making a directory or file.
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    for _ in 0..COLLISION_ATTEMPTS {
        let number = highest(dir)?
            .map_or(0, |(number, _)| number)
            .checked_add(1)
            .ok_or("Macro label sequence exhausted")?;
        let path = dir.join(format!("label-{number:020}.json"));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        return Ok(path);
    }
    Err("Macro label save collided repeatedly with another writer".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn names() -> Vec<String> {
        (0..SLOT_COUNT)
            .map(|slot| format!("Macro {}", slot + 1))
            .collect()
    }
    fn unique_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "byakko-macro-label-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn codec_roundtrip_and_invalid_fields() {
        let labels = Labels::new(names()).unwrap();
        assert_eq!(decode(&encode(&labels).unwrap()).unwrap(), labels);
        assert!(Labels::new(vec![]).is_err());
        let mut invalid = labels.clone();
        invalid.version = 2;
        assert!(encode(&invalid).is_err());
        invalid.version = 1;
        invalid.backend_id = "other".into();
        assert!(encode(&invalid).is_err());
        invalid.backend_id = BACKEND_ID.into();
        invalid.names[0] = "é".repeat(129);
        assert!(encode(&invalid).is_err());
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode(&labels).unwrap()).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        assert!(decode(b"{").is_err());
        assert!(decode(&vec![b' '; MAX_FILE_BYTES + 1]).is_err());
    }

    #[test]
    fn immutable_save_and_latest_load() {
        let dir = unique_dir();
        let first = Labels::new(names()).unwrap();
        let path1 = save_new(&dir, &first).unwrap();
        assert!(path1.ends_with("label-00000000000000000001.json"));
        let mut second = first.clone();
        second.names[49] = "Last slot".into();
        let path2 = save_new(&dir, &second).unwrap();
        assert!(path2.ends_with("label-00000000000000000002.json"));
        assert_eq!(load_latest(&dir).unwrap(), Some(second));
        assert_eq!(decode(&fs::read(path1).unwrap()).unwrap(), first);
    }

    #[test]
    fn corrupt_latest_does_not_fall_back() {
        let dir = unique_dir();
        let labels = Labels::new(names()).unwrap();
        save_new(&dir, &labels).unwrap();
        fs::write(dir.join("label-00000000000000000002.json"), b"truncated").unwrap();
        fs::write(dir.join("unrelated.json"), b"ignored").unwrap();
        assert!(load_latest(&dir).is_err());
    }
}
