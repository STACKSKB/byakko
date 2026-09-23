//! Append-only local macro labels. Names are preferences, never device state.
use byakko_core::macros::Choice;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;
const MAX_NAME_BYTES: usize = 256;
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024;
const PREFIX: &str = "label-";
const SUFFIX: &str = ".json";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    backend_id: String,
    names: StoredNames,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum StoredNames {
    ById(BTreeMap<String, String>),
    /// Version 1 of the retained local app stored names in slot order.
    Positional(Vec<String>),
}

fn validate(
    backend_id: &str,
    slots: &[Choice],
    names: &BTreeMap<String, String>,
) -> Result<(), String> {
    if backend_id.is_empty() || slots.is_empty() {
        return Err("Macro label backend and slot catalog must be nonempty".into());
    }
    let ids: BTreeSet<_> = slots.iter().map(|slot| slot.id.as_str()).collect();
    if ids.len() != slots.len() || ids.contains("") {
        return Err("Macro label slot catalog has an empty or duplicate ID".into());
    }
    if names.len() != slots.len()
        || !names.keys().all(|id| ids.contains(id.as_str()))
        || names.values().any(|name| name.len() > MAX_NAME_BYTES)
    {
        return Err(
            "Macro labels must match every slot and use at most 256 UTF-8 bytes per name".into(),
        );
    }
    Ok(())
}

fn encode(
    backend_id: &str,
    slots: &[Choice],
    names: &BTreeMap<String, String>,
) -> Result<Vec<u8>, String> {
    validate(backend_id, slots, names)?;
    let mut bytes = serde_json::to_vec_pretty(&Snapshot {
        version: VERSION,
        backend_id: backend_id.into(),
        names: StoredNames::ById(names.clone()),
    })
    .map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("Macro label snapshot exceeds 32 KiB".into());
    }
    Ok(bytes)
}

fn sequence(name: &str) -> Result<Option<u64>, String> {
    let Some(digits) = name
        .strip_prefix(PREFIX)
        .and_then(|name| name.strip_suffix(SUFFIX))
    else {
        return Ok(None);
    };
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    digits
        .parse()
        .map(Some)
        .map_err(|_| "Macro label snapshot sequence exceeds u64".into())
}

fn latest(directory: &Path) -> Result<Option<(u64, PathBuf)>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut newest = None;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(number) = sequence(&name)? else {
            continue;
        };
        if newest
            .as_ref()
            .is_none_or(|(previous, _)| number > *previous)
        {
            newest = Some((number, entry.path()));
        }
    }
    Ok(newest)
}

/// Load only the newest numbered snapshot. A damaged newest file is an error.
/// The caller supplies the backend and complete slot catalog for exact validation.
pub fn load_latest(
    directory: &Path,
    backend_id: &str,
    slots: &[Choice],
) -> Result<Option<BTreeMap<String, String>>, String> {
    let Some((_, path)) = latest(directory)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    File::open(&path)
        .and_then(|file| {
            file.take(MAX_SNAPSHOT_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| {
            format!(
                "Could not read newest macro labels {}: {error}",
                path.display()
            )
        })?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(format!(
            "Newest macro labels {} exceed 32 KiB",
            path.display()
        ));
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "Newest macro labels {} are invalid: {error}",
            path.display()
        )
    })?;
    if snapshot.version != VERSION || snapshot.backend_id != backend_id {
        return Err(format!(
            "Newest macro labels {} have a different version or backend",
            path.display()
        ));
    }
    let names = match snapshot.names {
        StoredNames::ById(names) => names,
        StoredNames::Positional(values) => {
            if values.len() != slots.len() {
                return Err(format!(
                    "Newest macro labels {} have a different slot count",
                    path.display()
                ));
            }
            slots
                .iter()
                .map(|slot| slot.id.clone())
                .zip(values)
                .collect()
        }
    };
    validate(backend_id, slots, &names).map_err(|error| {
        format!(
            "Newest macro labels {} are invalid: {error}",
            path.display()
        )
    })?;
    Ok(Some(names))
}

/// Write a complete snapshot, then publish it under a new numbered name.
/// Publishing uses an exclusive hard link, so readers see either complete
/// bytes or no new snapshot even when another process saves concurrently.
pub fn save_new(
    directory: &Path,
    backend_id: &str,
    slots: &[Choice],
    names: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let bytes = encode(backend_id, slots, names)?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = directory.join(format!(
        ".label-pending-{}-{stamp}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    drop(file);
    let result = (|| {
        for _ in 0..16 {
            let next = latest(directory)?
                .map_or(Some(1), |(number, _)| number.checked_add(1))
                .ok_or("Macro label snapshot sequence exhausted")?;
            let path = directory.join(format!("{PREFIX}{next:020}{SUFFIX}"));
            match fs::hard_link(&temporary, &path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("Concurrent macro label saves did not settle".into())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<Choice> {
        (0..50)
            .map(|index| Choice {
                id: format!("slot-{index:02}"),
                label: format!("Macro {}", index + 1),
            })
            .collect()
    }

    fn names(slots: &[Choice]) -> BTreeMap<String, String> {
        slots
            .iter()
            .map(|slot| (slot.id.clone(), slot.label.clone()))
            .collect()
    }

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "byakko-label-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn round_trip_fifty_slots_and_reject_invalid_names() {
        let directory = directory();
        let slots = catalog();
        let mut labels = names(&slots);
        assert_eq!(load_latest(&directory, "nia87", &slots).unwrap(), None);
        let first = save_new(&directory, "nia87", &slots, &labels).unwrap();
        assert!(first.ends_with("label-00000000000000000001.json"));
        assert_eq!(
            load_latest(&directory, "nia87", &slots).unwrap(),
            Some(labels.clone())
        );
        labels.insert("slot-49".into(), "新しい名前".into());
        let second = save_new(&directory, "nia87", &slots, &labels).unwrap();
        assert!(second.ends_with("label-00000000000000000002.json"));
        assert_eq!(
            load_latest(&directory, "nia87", &slots).unwrap(),
            Some(labels.clone())
        );
        assert!(first.exists());
        labels.insert("slot-49".into(), "é".repeat(129));
        assert!(save_new(&directory, "nia87", &slots, &labels).is_err());
        assert_eq!(
            load_latest(&directory, "nia87", &slots).unwrap().unwrap()["slot-49"],
            "新しい名前"
        );
    }

    #[test]
    fn corrupt_newest_does_not_fall_back() {
        let directory = directory();
        let slots = catalog();
        let labels = names(&slots);
        save_new(&directory, "nia87", &slots, &labels).unwrap();
        fs::write(directory.join("label-00000000000000000002.json"), b"{").unwrap();
        assert!(
            load_latest(&directory, "nia87", &slots)
                .unwrap_err()
                .contains("Newest macro labels")
        );
        assert!(
            save_new(&directory, "nia87", &slots, &labels)
                .unwrap()
                .ends_with("label-00000000000000000003.json")
        );
        fs::write(directory.join("label-99999999999999999999.json"), b"{}").unwrap();
        assert!(
            load_latest(&directory, "nia87", &slots)
                .unwrap_err()
                .contains("sequence exceeds u64")
        );
    }

    #[test]
    fn positional_legacy_snapshot_uses_supplied_slot_order() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let mut slots = catalog();
        slots.reverse();
        let positional: Vec<_> = slots.iter().map(|slot| slot.label.clone()).collect();
        let legacy = serde_json::json!({
            "version": 1,
            "backend_id": "nia87",
            "names": positional,
        });
        fs::write(
            directory.join("label-00000000000000000001.json"),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let loaded = load_latest(&directory, "nia87", &slots).unwrap().unwrap();
        assert_eq!(loaded["slot-49"], "Macro 50");
        assert_eq!(loaded["slot-00"], "Macro 1");
        let new_path = save_new(&directory, "nia87", &slots, &loaded).unwrap();
        let new: serde_json::Value = serde_json::from_slice(&fs::read(new_path).unwrap()).unwrap();
        assert!(new["names"].is_object());
        assert_eq!(
            load_latest(&directory, "nia87", &slots).unwrap(),
            Some(loaded)
        );
    }
}
