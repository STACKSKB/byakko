// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-user appearance preferences; no keyboard or Iced state enters this file.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "appearance.json";
const MAX_BYTES: u64 = 8 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    #[default]
    Default,
    Slate,
    Forest,
    #[serde(rename = "stackskb")]
    StacksKB,
    Custom,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub canvas: [u8; 3],
    pub surface: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub accent: [u8; 3],
    pub rule: [u8; 3],
    pub focus: [u8; 3],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub mode: Mode,
    pub palette: Palette,
    /// Custom colors in light, then dark order.
    pub custom: [Colors; 2],
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: Mode::Dark,
            palette: Palette::Default,
            custom: [
                Colors {
                    canvas: [0xf4, 0xf3, 0xef],
                    surface: [0xff, 0xff, 0xff],
                    text: [0x27, 0x28, 0x23],
                    muted: [0x68, 0x69, 0x62],
                    accent: [0x56, 0x5b, 0x8f],
                    rule: [0xd8, 0xd8, 0xd0],
                    focus: [0x56, 0x5b, 0x8f],
                },
                Colors {
                    canvas: [0x20, 0x21, 0x1f],
                    surface: [0x29, 0x2b, 0x28],
                    text: [0xe5, 0xe6, 0xdf],
                    muted: [0xad, 0xb0, 0xa6],
                    accent: [0xb5, 0xb9, 0xec],
                    rule: [0x40, 0x44, 0x3b],
                    focus: [0xb5, 0xb9, 0xec],
                },
            ],
        }
    }
}

/// Missing preferences use the caller's default; invalid files remain errors.
pub fn load(directory: &Path) -> Result<Option<Preferences>, String> {
    let path = directory.join(FILE);
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not open {}: {error}", path.display())),
    };
    if !file
        .metadata()
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?
        .is_file()
    {
        return Err(format!("{} is not a regular file", path.display()));
    }
    super::json::read_json(file, MAX_BYTES)
        .map(Some)
        .map_err(|error| format!("Invalid appearance preferences {}: {error}", path.display()))
}

/// Write a synced sibling, then replace the visible preference file in one rename.
pub fn save(directory: &Path, preferences: &Preferences) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(preferences).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Appearance preferences exceed 8 KiB".into());
    }
    fs::create_dir_all(directory)
        .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = directory.join(format!(
        ".appearance-pending-{}-{stamp}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, directory.join(FILE))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("Could not save appearance preferences: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "byakko-appearance-test-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn clean(directory: &Path) {
        fs::remove_file(directory.join(FILE)).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn missing_roundtrip_and_atomic_replacement() {
        let directory = directory();
        assert_eq!(load(&directory).unwrap(), None);
        let initial = Preferences::default();
        save(&directory, &initial).unwrap();
        assert_eq!(load(&directory).unwrap(), Some(initial.clone()));

        let mut changed = initial;
        changed.mode = Mode::Light;
        changed.palette = Palette::StacksKB;
        changed.custom[0].accent = [4, 5, 6];
        save(&directory, &changed).unwrap();
        assert_eq!(load(&directory).unwrap(), Some(changed));
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            1,
            "replacement leaves no pending file"
        );
        clean(&directory);
    }

    #[test]
    fn malformed_oversize_and_bad_rgb_are_errors() {
        let directory = directory();
        fs::create_dir(&directory).unwrap();
        let path = directory.join(FILE);
        fs::write(&path, b"{broken").unwrap();
        assert!(load(&directory).is_err());
        fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(load(&directory).is_err());

        let mut invalid = serde_json::to_value(Preferences::default()).unwrap();
        invalid["custom"][0]["canvas"] = serde_json::json!([256, 0, 0]);
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(load(&directory).is_err());
        invalid["custom"][0]["canvas"] = serde_json::json!([1, 2]);
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(load(&directory).is_err());
        clean(&directory);
    }
}
