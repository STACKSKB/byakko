//! File-based macro programming through the portable session contract.

use byakko_core::{
    macros::{self, Content, Program, Snapshot, editor::Status as MacroStatus},
    session::{Acceptance, FileOperation, Session},
};
use byakko_devices::Executor;

/// Compare a complete snapshot file with the freshly read selected slot.
/// A stored repeat count of zero may be inspected but cannot authorize a write.
pub fn plan_macro(session: &Session, target: &Snapshot) -> Result<Option<Program>, String> {
    let editor = session.macros().ok_or("Device does not support macros")?;
    if *editor.status() != MacroStatus::Ready {
        return Err("Read and verify the macro slot before planning changes".into());
    }
    let current = editor.baseline().ok_or("Verified macro has no baseline")?;
    if editor.slot() != target.slot
        || current.backend_id != target.backend_id
        || current.revision != target.revision
    {
        return Err(
            "Macro file slot, backend or revision differs from the connected keyboard; read again"
                .into(),
        );
    }
    let (Content::Editable(before), Content::Editable(after)) = (&current.content, &target.content)
    else {
        return Err("Opaque macros are available only as raw backups".into());
    };
    macros::validate_program(editor.capabilities(), after)?;
    if before == after {
        return Ok(None);
    }
    if !editor
        .capabilities()
        .editable_repeat_counts
        .contains(&after.repeat_count)
    {
        return Err("Stage a supported repeat count before saving this macro".into());
    }
    Ok(Some(after.clone()))
}

/// Stage a reviewed file through a correlated import, then wait for backup,
/// write and verified complete readback on the selected device executor.
pub fn apply_macro(
    session: &mut Session,
    executor: &Executor,
    target: &Snapshot,
) -> Result<Snapshot, String> {
    let desired = plan_macro(session, target)?.ok_or("Macro file contains no changes")?;
    let ticket = session.begin_macro_file(FileOperation::Import)?;
    if session.finish_macro_file(&ticket, Some(desired))? != Acceptance::Accepted {
        return Err("Stale macro file operation".into());
    }
    let command = session.request_macro_apply()?;
    super::submit_and_wait(session, executor, command, None)?;
    let editor = session.macros().ok_or("Device does not support macros")?;
    match editor.status() {
        MacroStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified macro apply has no baseline".into()),
        status => Err(format!("Macro apply did not verify: {status:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_macro;
    use byakko_core::{
        Action, Descriptor, Layer, PhysicalKey, State,
        macros::{Action as MacroAction, Capabilities, Choice, Event},
    };
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf, time::Duration};

    fn fixture(repeat_count: u32, opaque: bool) -> (Session, Executor, Snapshot) {
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
            slots: vec![Choice {
                id: "slot-00".into(),
                label: "Macro 1".into(),
            }],
            repeat_counts: 0..=10,
            editable_repeat_counts: 1..=10,
            delays_ms: 0..=100,
            keys: Some(4..=5),
            buttons: vec![],
            movement: None,
            backend_actions: vec![],
            bindings: vec![],
            byte_budget: None,
        };
        let snapshot = Snapshot {
            backend_id: "memory".into(),
            slot: "slot-00".into(),
            revision: vec![2, 3],
            content: if opaque {
                Content::Opaque {
                    reason: "Unknown encoding".into(),
                }
            } else {
                Content::Editable(Program {
                    repeat_count,
                    events: vec![],
                })
            },
        };
        let device = MemoryDevice::new(descriptor.clone(), state)
            .unwrap()
            .with_macros(capabilities.clone(), vec![snapshot.clone()])
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let session = Session::new(descriptor)
            .unwrap()
            .with_macros(capabilities)
            .unwrap();
        (session, executor, snapshot)
    }

    fn ready(repeat_count: u32, opaque: bool) -> (Session, Executor, Snapshot) {
        let (mut session, executor, expected) = fixture(repeat_count, opaque);
        let actual =
            read_macro(&mut session, &executor, "slot-00", Duration::from_secs(1)).unwrap();
        assert_eq!(actual, expected);
        (session, executor, actual)
    }

    fn changed(snapshot: &Snapshot) -> Snapshot {
        let mut target = snapshot.clone();
        target.content = Content::Editable(Program {
            repeat_count: 2,
            events: vec![Event {
                action: MacroAction::Key {
                    usage: 4,
                    pressed: true,
                },
                delay_ms: 50,
            }],
        });
        target
    }

    #[test]
    fn applies_reviewed_program_and_rereads_memory_device() {
        let (mut session, executor, current) = ready(1, false);
        assert_eq!(plan_macro(&session, &current).unwrap(), None);
        let target = changed(&current);
        let desired = plan_macro(&session, &target).unwrap().unwrap();
        assert_eq!(
            session.macros().unwrap().draft(),
            Some(&Program {
                repeat_count: 1,
                events: vec![]
            })
        );
        let applied = apply_macro(&mut session, &executor, &target).unwrap();
        assert_eq!(applied.content, Content::Editable(desired));
        assert_ne!(applied.revision, current.revision);
        assert_eq!(
            read_macro(&mut session, &executor, "slot-00", Duration::from_secs(1)).unwrap(),
            applied
        );
    }

    #[test]
    fn rejects_stale_opaque_malformed_and_zero_without_draft_mutation() {
        let (mut session, executor, current) = ready(1, false);
        let mut stale = changed(&current);
        stale.revision.push(9);
        assert!(plan_macro(&session, &stale).is_err());
        let mut foreign = changed(&current);
        foreign.backend_id = "other".into();
        assert!(plan_macro(&session, &foreign).is_err());
        let mut wrong_slot = changed(&current);
        wrong_slot.slot = "slot-01".into();
        assert!(plan_macro(&session, &wrong_slot).is_err());
        let mut invalid = changed(&current);
        let Content::Editable(program) = &mut invalid.content else {
            unreachable!()
        };
        program.events[0].delay_ms = 101;
        assert!(plan_macro(&session, &invalid).is_err());
        let mut zero = changed(&current);
        let Content::Editable(program) = &mut zero.content else {
            unreachable!()
        };
        program.repeat_count = 0;
        assert!(plan_macro(&session, &zero).is_err());
        assert!(apply_macro(&mut session, &executor, &zero).is_err());
        assert_eq!(session.macros().unwrap().baseline(), Some(&current));
        assert_eq!(
            session.macros().unwrap().draft(),
            Some(&Program {
                repeat_count: 1,
                events: vec![]
            })
        );

        let (session, _, opaque) = ready(1, true);
        assert!(plan_macro(&session, &opaque).is_err());
        let (session, _, zero_baseline) = ready(0, false);
        assert_eq!(plan_macro(&session, &zero_baseline).unwrap(), None);
        let mut dirty_zero = zero_baseline;
        dirty_zero.content = Content::Editable(Program {
            repeat_count: 0,
            events: vec![Event {
                action: MacroAction::Key {
                    usage: 4,
                    pressed: true,
                },
                delay_ms: 1,
            }],
        });
        assert!(plan_macro(&session, &dirty_zero).is_err());
    }
}
