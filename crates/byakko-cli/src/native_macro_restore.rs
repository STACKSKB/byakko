//! Explicit Nia87 native-backup recovery, separate from portable macro editing.
use super::{Mode, read_bounded};
use byakko_devices::nia87;
use serde::Deserialize;
use std::path::Path;

/// The exact slot before-image written by the guarded Nia87 macro transaction.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeMacroBackup {
    slot: u8,
    bytes: Vec<u8>,
}

impl NativeMacroBackup {
    fn program(&self) -> Result<nia87::macros::Macro, Box<dyn std::error::Error>> {
        if self.slot >= 50 {
            return Err("Native macro backup slot is outside 0..49".into());
        }
        let program = nia87::macros::decode(&self.bytes)?;
        if nia87::macros::encode(&program)? != self.bytes {
            return Err("Native macro backup is not an exact codec round trip".into());
        }
        Ok(program)
    }
}

pub(super) fn load(path: &str) -> Result<NativeMacroBackup, Box<dyn std::error::Error>> {
    let backup: NativeMacroBackup = serde_json::from_slice(&read_bounded(
        Path::new(path),
        64 * 1024,
        "Native macro backup exceeds the 64 KiB JSON limit",
    )?)?;
    backup.program()?;
    Ok(backup)
}

pub(super) fn run(
    mode: Mode,
    backup: &NativeMacroBackup,
    target: nia87::device::Target,
    backup_dir: &Path,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let desired = backup.program()?;
    let access = nia87::device::Access::bound(target);
    let current = access
        .read_macro(backup.slot)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let before = nia87::macros::decode(&current)?;
    let changed = current != backup.bytes;
    if mode == Mode::Apply && changed {
        eprintln!("Restoring macro slot {} on {path}", backup.slot);
        eprintln!("Before-image backup directory: {}", backup_dir.display());
        let verified = access
            .apply_macro_detailed(backup.slot, &current, &desired, backup_dir)
            .map_err(|failure| {
                std::io::Error::other(format!(
                    "{}; recovery {:?}",
                    failure.message, failure.recovery
                ))
            })?;
        if verified != backup.bytes {
            return Err("Macro restoration did not match its exact native backup".into());
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "slot": backup.slot,
            "changed": changed,
            "before": before,
            "after": desired,
            "applied": mode == Mode::Apply && changed,
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_empty_backup_is_restorable_but_noncanonical_bytes_are_not() {
        let mut backup = NativeMacroBackup {
            slot: 49,
            bytes: vec![0; 256],
        };
        let program = backup.program().unwrap();
        assert_eq!(program.repeat_count, 0);
        assert!(program.events.is_empty());
        backup.bytes[255] = 1;
        assert!(backup.program().is_err());
        backup.bytes[255] = 0;
        backup.slot = 50;
        assert!(backup.program().is_err());
    }
}
