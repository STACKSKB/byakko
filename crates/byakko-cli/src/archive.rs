//! Diagnostic capture/export and offline comparison. No archive restore intent.
use super::execute;
use byakko_core::{
    model::archive::{NativeArchive, SectionChange},
    session::{Connection, Outcome, Session},
    validation,
};
use byakko_devices::{Executor, storage};
use std::{path::Path, time::Duration};
pub const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
pub fn capture_archive(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<NativeArchive, String> {
    if matches!(session.connection(), Connection::Disconnected) {
        executor.set_generation(session.connect()?);
    }
    let command = session.capture_archive()?;
    match execute(session, executor, command, Some(timeout))? {
        Outcome::ArchiveCaptured => Ok(session.archive().unwrap().captured().unwrap().clone()),
        outcome => Err(format!("Diagnostic capture did not complete: {outcome:?}")),
    }
}
pub fn export_archive(path: &Path, archive: &NativeArchive) -> Result<(), String> {
    storage::save_new_json(path, archive)
}
pub fn compare_archives(
    before: &NativeArchive,
    target: &NativeArchive,
) -> Result<Vec<SectionChange>, String> {
    if before.backend_id != target.backend_id || before.format_id != target.format_id {
        return Err("Archives belong to different backends or formats".into());
    }
    match (before.backend_id.as_str(), before.format_id.as_str()) {
        ("nia87", _) => byakko_devices::nia87::archive_adapter::compare(before, target),
        ("memory", "memory-demo-v1") => {
            let caps = byakko_core::model::archive::ArchiveCapabilities {
                backend_id: "memory".into(),
                format_id: "memory-demo-v1".into(),
                max_bytes: 1024 * 1024,
            };
            validation::archive::validate_archive(&caps, before)?;
            validation::archive::validate_archive(&caps, target)?;
            let before: serde_json::Value =
                serde_json::from_slice(&before.bytes).map_err(|error| error.to_string())?;
            let target: serde_json::Value =
                serde_json::from_slice(&target.bytes).map_err(|error| error.to_string())?;
            let fields = [
                ("keymap", "Key bindings"),
                ("macros", "Macro slots"),
                ("lighting", "Global lighting"),
                ("picture", "Per-key colors"),
                ("settings", "Settings"),
            ];
            let mut changes = Vec::new();
            for (id, label) in fields {
                let a = before.get(id).ok_or("Incomplete demo archive")?;
                let b = target.get(id).ok_or("Incomplete demo archive")?;
                if a != b {
                    changes.push(SectionChange {
                        id: id.into(),
                        label: label.into(),
                        count: None,
                    });
                }
            }
            Ok(changes)
        }
        _ => Err("No offline comparison is available for this archive format".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::settings;
    #[test]
    fn capture_exports_and_compares_without_restore() {
        let device = byakko_devices::memory::demo().unwrap();
        let mut session = device.session().unwrap();
        let worker = Executor::spawn(device, Default::default()).unwrap();
        let timeout = Duration::from_secs(2);
        let before = capture_archive(&mut session, &worker, timeout).unwrap();
        super::super::read_settings(&mut session, &worker, timeout).unwrap();
        session
            .edit_settings(settings::Edit {
                id: "sleep".into(),
                value: settings::Value::Number(3),
            })
            .unwrap();
        let command = session.save_settings().unwrap();
        assert!(matches!(
            execute(&mut session, &worker, command, None).unwrap(),
            Outcome::SettingsSaved
        ));
        let after = capture_archive(&mut session, &worker, timeout).unwrap();
        assert_eq!(
            compare_archives(&before, &after)
                .unwrap()
                .iter()
                .map(|change| change.id.as_str())
                .collect::<Vec<_>>(),
            vec!["settings"]
        );
        assert!(compare_archives(&before, &before).unwrap().is_empty());
        let path =
            std::env::temp_dir().join(format!("byakko-cli-capture-{}.json", std::process::id()));
        export_archive(&path, &after).unwrap();
        assert!(export_archive(&path, &before).is_err());
        assert_eq!(
            storage::load_json::<NativeArchive>(&path, MAX_JSON_BYTES).unwrap(),
            after
        );
        std::fs::remove_file(path).unwrap();
    }
}
