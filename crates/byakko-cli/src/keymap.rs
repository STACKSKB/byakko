//! File-based keymap programming through the shared session contract.

use byakko_core::{
    Change, State,
    session::{Session, Status},
    validate_changes, validate_state,
};
use byakko_devices::Executor;

/// Compare a proposed complete state with a freshly read device baseline.
/// The opaque revision is an exact before-image token, not editable key data.
pub fn plan_keymap(session: &Session, target: &State) -> Result<Vec<Change>, String> {
    if *session.status() != Status::Ready {
        return Err("Read and verify the keymap before planning changes".into());
    }
    let current = session
        .baseline()
        .ok_or("Verified keymap has no baseline")?;
    if current.revision != target.revision {
        return Err("Keymap file revision differs from the connected keyboard; read again".into());
    }
    validate_state(session.descriptor(), target)?;
    let changes = target
        .bindings
        .iter()
        .flat_map(|(layer, bindings)| {
            bindings
                .iter()
                .filter(|(key, action)| current.bindings[layer].get(*key) != Some(*action))
                .map(move |(key, action)| Change {
                    layer: layer.clone(),
                    key: key.clone(),
                    action: action.clone(),
                })
        })
        .collect::<Vec<_>>();
    validate_changes(session.descriptor(), &changes)?;
    Ok(changes)
}

/// Stage one reviewed file and wait for the backend's backup/write/readback
/// transaction to finish. A write is never abandoned on a CLI read timeout.
pub fn apply_keymap(
    session: &mut Session,
    executor: &Executor,
    target: &State,
) -> Result<State, String> {
    let changes = plan_keymap(session, target)?;
    if changes.is_empty() {
        return Err("Keymap file contains no changes".into());
    }
    for change in changes {
        session.stage(change)?;
    }
    let command = session.request_apply()?;
    super::submit_and_wait(session, executor, command, None)?;
    match session.status() {
        Status::Ready => session
            .baseline()
            .cloned()
            .ok_or("Verified apply has no baseline".into()),
        status => Err(format!("Keymap apply did not verify: {status:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_keymap;
    use byakko_core::{Action, ActionChoice, Descriptor, Layer, PhysicalKey};
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf, time::Duration};

    #[test]
    fn plan_and_apply_use_the_same_session_and_reject_stale_revisions() {
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
            actions: [4, 5]
                .map(|usage| ActionChoice {
                    label: format!("Key {usage}"),
                    action: Action::Key(usage),
                })
                .to_vec(),
            shortcuts: None,
        };
        let initial = State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([("one".into(), Action::Key(4))]),
            )]),
        };
        let device = MemoryDevice::new(descriptor.clone(), initial.clone()).unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor).unwrap();
        let current = read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert!(
            apply_keymap(&mut session, &executor, &current)
                .unwrap_err()
                .contains("no changes")
        );
        let mut target = current.clone();
        target
            .bindings
            .get_mut("base")
            .unwrap()
            .insert("one".into(), Action::Key(5));
        assert_eq!(plan_keymap(&session, &target).unwrap().len(), 1);
        let actual = apply_keymap(&mut session, &executor, &target).unwrap();
        assert_eq!(actual.bindings, target.bindings);
        assert_ne!(actual.revision, current.revision);
        assert!(
            plan_keymap(&session, &target)
                .unwrap_err()
                .contains("revision")
        );
        assert!(
            apply_keymap(&mut session, &executor, &target)
                .unwrap_err()
                .contains("revision")
        );
        let mut malformed = actual.clone();
        malformed.bindings.get_mut("base").unwrap().remove("one");
        assert!(plan_keymap(&session, &malformed).is_err());
    }
}
