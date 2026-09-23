//! File-based scalar settings through the portable one-field session contract.

use byakko_core::{
    session::{Session, Status},
    settings::{self, Content, Edit, Snapshot, editor::Status as SettingsStatus},
};
use byakko_devices::Executor;

/// Compare a complete editable file with a freshly verified settings read.
/// The raw revision is an exact before-image token, not editable setting data.
pub fn plan_settings(session: &Session, target: &Snapshot) -> Result<Vec<Edit>, String> {
    if *session.status() != Status::Ready {
        return Err("Read and verify the keymap before planning settings".into());
    }
    let editor = session
        .settings()
        .ok_or("Device does not support settings")?;
    if *editor.status() != SettingsStatus::Ready {
        return Err("Read and verify settings before planning changes".into());
    }
    let current = editor
        .baseline()
        .ok_or("Verified settings have no baseline")?;
    if current.backend_id != target.backend_id || current.revision != target.revision {
        return Err(
            "Settings file backend or revision differs from the connected keyboard; read again"
                .into(),
        );
    }
    settings::validate_snapshot(editor.capabilities(), target)?;
    let (Content::Editable(before), Content::Editable(after)) = (&current.content, &target.content)
    else {
        return Err("Opaque settings are available only as a raw backup".into());
    };
    let changes = after
        .iter()
        .filter(|(id, value)| before.get(*id) != Some(*value))
        .map(|(id, value)| Edit {
            id: id.clone(),
            value: value.clone(),
        })
        .collect::<Vec<_>>();
    if changes.len() > 1 {
        return Err("Settings file changes multiple fields; apply one field at a time".into());
    }
    Ok(changes)
}

/// Stage one field and wait for the backend's backup, write, and full readback.
pub fn apply_settings(
    session: &mut Session,
    executor: &Executor,
    target: &Snapshot,
) -> Result<Snapshot, String> {
    let [edit] = plan_settings(session, target)?
        .try_into()
        .map_err(|_| "Settings file must change exactly one field")?;
    session.edit_setting(edit)?;
    let command = session.request_setting_apply()?;
    super::submit_and_wait(session, executor, command, None)?;
    let editor = session
        .settings()
        .ok_or("Device does not support settings")?;
    match editor.status() {
        SettingsStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified settings apply has no baseline".into()),
        status => Err(format!("Settings apply did not verify: {status:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_keymap, read_settings};
    use byakko_core::{
        Action, Descriptor, Layer, PhysicalKey, State,
        settings::{Capabilities, Field, Kind, Value},
    };
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf, time::Duration};

    fn fixture() -> (Session, Executor) {
        let descriptor = Descriptor {
            backend_id: "memory".into(),
            device_name: "One key".into(),
            keys: vec![PhysicalKey {
                id: "one".into(),
                label: "One".into(),
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                visible: true,
                writable: true,
            }],
            layers: vec![Layer {
                id: "base".into(),
                label: "Base".into(),
            }],
            actions: vec![],
            shortcuts: None,
        };
        let state = State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([("one".into(), Action::Key(4))]),
            )]),
        };
        let capabilities = Capabilities {
            backend_id: "memory".into(),
            fields: vec![
                Field {
                    id: "enabled".into(),
                    label: "Enabled".into(),
                    kind: Kind::Toggle,
                },
                Field {
                    id: "timer".into(),
                    label: "Timer".into(),
                    kind: Kind::Number {
                        min: 10,
                        max: 60,
                        step: 5,
                        unit: "min".into(),
                        disabled_zero: true,
                    },
                },
            ],
        };
        let settings = Snapshot {
            backend_id: "memory".into(),
            revision: vec![2],
            content: Content::Editable(BTreeMap::from([
                ("enabled".into(), Value::Toggle(false)),
                ("timer".into(), Value::Number(10)),
            ])),
        };
        let device = MemoryDevice::new(descriptor.clone(), state)
            .unwrap()
            .with_settings(capabilities.clone(), settings)
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let session = Session::new(descriptor)
            .unwrap()
            .with_settings(capabilities)
            .unwrap();
        (session, executor)
    }

    fn ready() -> (Session, Executor, Snapshot) {
        let (mut session, executor) = fixture();
        read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        let current = read_settings(&mut session, &executor, Duration::from_secs(1)).unwrap();
        (session, executor, current)
    }

    fn values(snapshot: &mut Snapshot) -> &mut BTreeMap<String, Value> {
        let Content::Editable(values) = &mut snapshot.content else {
            panic!("expected editable settings")
        };
        values
    }

    #[test]
    fn plans_one_field_and_applies_through_verified_memory_readback() {
        let (mut session, executor, current) = ready();
        assert!(plan_settings(&session, &current).unwrap().is_empty());
        assert!(apply_settings(&mut session, &executor, &current).is_err());
        let mut target = current.clone();
        values(&mut target).insert("timer".into(), Value::Number(25));
        assert_eq!(
            plan_settings(&session, &target).unwrap(),
            vec![Edit {
                id: "timer".into(),
                value: Value::Number(25)
            }]
        );
        let applied = apply_settings(&mut session, &executor, &target).unwrap();
        assert_eq!(
            applied.content,
            Content::Editable(BTreeMap::from([
                ("enabled".into(), Value::Toggle(false)),
                ("timer".into(), Value::Number(25)),
            ]))
        );
        assert_ne!(applied.revision, current.revision);
        assert_eq!(session.settings().unwrap().baseline(), Some(&applied));
        assert!(plan_settings(&session, &target).is_err());
        read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert_eq!(
            read_settings(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            applied
        );
    }

    #[test]
    fn rejects_stale_multi_opaque_and_malformed_files_without_staging() {
        let (mut session, executor, current) = ready();
        let mut stale = current.clone();
        stale.revision.push(9);
        assert!(
            plan_settings(&session, &stale)
                .unwrap_err()
                .contains("revision")
        );
        let mut foreign = current.clone();
        foreign.backend_id = "other".into();
        assert!(plan_settings(&session, &foreign).is_err());
        let mut multiple = current.clone();
        values(&mut multiple).insert("timer".into(), Value::Number(20));
        values(&mut multiple).insert("enabled".into(), Value::Toggle(true));
        assert!(
            plan_settings(&session, &multiple)
                .unwrap_err()
                .contains("multiple")
        );
        let mut malformed = current.clone();
        values(&mut malformed).remove("timer");
        assert!(plan_settings(&session, &malformed).is_err());
        let mut invalid = current.clone();
        values(&mut invalid).insert("timer".into(), Value::Number(11));
        assert!(plan_settings(&session, &invalid).is_err());
        let mut opaque = current.clone();
        opaque.content = Content::Opaque {
            reason: "unknown".into(),
        };
        assert!(plan_settings(&session, &opaque).is_err());
        assert!(apply_settings(&mut session, &executor, &multiple).is_err());
        let editor = session.settings().unwrap();
        assert_eq!(editor.status(), &SettingsStatus::Ready);
        assert_eq!(editor.baseline(), Some(&current));
        assert_eq!(
            editor.draft(),
            match &current.content {
                Content::Editable(values) => Some(values),
                _ => None,
            }
        );
    }
}
