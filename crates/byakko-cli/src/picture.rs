//! File-based per-key colors through the portable picture session contract.

use byakko_core::{
    picture::{self, Content, Edit, Snapshot, editor::Status as PictureStatus},
    session::{Session, Status},
};
use byakko_devices::Executor;

/// Compare a complete editable color file with a freshly verified USB read.
/// The revision is an exact before-image token, not editable RGB data.
pub fn plan_colors(session: &Session, target: &Snapshot) -> Result<Vec<Edit>, String> {
    if *session.status() != Status::Ready {
        return Err("Read and verify the keymap before planning colors".into());
    }
    let editor = session
        .picture()
        .ok_or("Device does not support per-key colors")?;
    if *editor.status() != PictureStatus::Ready {
        return Err("Read and verify colors before planning changes".into());
    }
    let current = editor
        .baseline()
        .ok_or("Verified colors have no baseline")?;
    if current.backend_id != target.backend_id
        || current.revision != target.revision
        || current.context_revision != target.context_revision
    {
        return Err("Color file or picture selector changed; read colors again".into());
    }
    picture::validate_snapshot(editor.capabilities(), target)?;
    let (Content::Editable(before), Content::Editable(after)) = (&current.content, &target.content)
    else {
        return Err("Opaque colors are available only as a raw backup".into());
    };
    Ok(after
        .iter()
        .filter(|(key, color)| before.get(*key) != Some(*color))
        .map(|(key, color)| Edit::Color {
            key: key.clone(),
            color: *color,
        })
        .collect())
}

/// Stage the changed advertised keys and wait for one guarded transaction.
pub fn apply_colors(
    session: &mut Session,
    executor: &Executor,
    target: &Snapshot,
) -> Result<Snapshot, String> {
    let changes = plan_colors(session, target)?;
    if changes.is_empty() {
        return Err("Color file contains no changes".into());
    }
    for change in changes {
        session.edit_picture(change)?;
    }
    let command = session.request_picture_apply()?;
    super::submit_and_wait(session, executor, command, None)?;
    let editor = session
        .picture()
        .ok_or("Device does not support per-key colors")?;
    match editor.status() {
        PictureStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified color apply has no baseline".into()),
        status => Err(format!("Color apply did not verify: {status:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_colors, read_keymap};
    use byakko_core::{Action, Descriptor, Layer, PhysicalKey, State, picture::Capabilities};
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf, time::Duration};

    fn ready(opaque: bool) -> (Session, Executor, Snapshot) {
        let descriptor = Descriptor {
            backend_id: "memory".into(),
            device_name: "Two keys".into(),
            keys: ["one", "two"]
                .map(|id| PhysicalKey {
                    id: id.into(),
                    label: id.into(),
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                    visible: true,
                    writable: true,
                })
                .to_vec(),
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
                BTreeMap::from([
                    ("one".into(), Action::Key(4)),
                    ("two".into(), Action::Key(5)),
                ]),
            )]),
        };
        let caps = Capabilities {
            backend_id: "memory".into(),
            keys: vec!["one".into(), "two".into()],
            lighting_effect: None,
        };
        let colors = Snapshot {
            backend_id: "memory".into(),
            revision: vec![2],
            context_revision: Vec::new(),
            content: if opaque {
                Content::Opaque {
                    reason: "Unknown bytes".into(),
                }
            } else {
                Content::Editable(BTreeMap::from([
                    ("one".into(), [1, 2, 3]),
                    ("two".into(), [4, 5, 6]),
                ]))
            },
        };
        let device = MemoryDevice::new(descriptor.clone(), state)
            .unwrap()
            .with_picture(caps.clone(), colors.clone())
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor)
            .unwrap()
            .with_picture(caps)
            .unwrap();
        read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        let current = read_colors(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert_eq!(current, colors);
        (session, executor, current)
    }

    fn edit(snapshot: &mut Snapshot, key: &str, color: [u8; 3]) {
        let Content::Editable(colors) = &mut snapshot.content else {
            panic!("expected editable colors")
        };
        colors.insert(key.into(), color);
    }

    #[test]
    fn plans_two_colors_applies_once_and_rereads_complete_map() {
        let (mut session, executor, current) = ready(false);
        assert!(plan_colors(&session, &current).unwrap().is_empty());
        let mut target = current.clone();
        edit(&mut target, "one", [7, 8, 9]);
        edit(&mut target, "two", [10, 11, 12]);
        assert_eq!(plan_colors(&session, &target).unwrap().len(), 2);
        let applied = apply_colors(&mut session, &executor, &target).unwrap();
        assert_eq!(applied.content, target.content);
        assert_ne!(applied.revision, current.revision);
        read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert_eq!(
            read_colors(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            applied
        );
    }

    #[test]
    fn rejects_stale_incomplete_extra_and_opaque_without_staging() {
        let (mut session, executor, current) = ready(false);
        let mut stale = current.clone();
        stale.revision.push(9);
        assert!(plan_colors(&session, &stale).is_err());
        let mut wrong_context = current.clone();
        wrong_context.context_revision.push(1);
        assert!(plan_colors(&session, &wrong_context).is_err());
        let mut foreign = current.clone();
        foreign.backend_id = "other".into();
        assert!(plan_colors(&session, &foreign).is_err());
        let mut missing = current.clone();
        let Content::Editable(colors) = &mut missing.content else {
            unreachable!()
        };
        colors.remove("one");
        assert!(plan_colors(&session, &missing).is_err());
        let mut extra = current.clone();
        edit(&mut extra, "other", [1, 1, 1]);
        assert!(plan_colors(&session, &extra).is_err());
        let mut opaque = current.clone();
        opaque.content = Content::Opaque {
            reason: "unknown".into(),
        };
        assert!(plan_colors(&session, &opaque).is_err());
        assert!(apply_colors(&mut session, &executor, &missing).is_err());
        assert!(apply_colors(&mut session, &executor, &current).is_err());
        assert_eq!(session.picture().unwrap().status(), &PictureStatus::Ready);
        assert_eq!(session.picture().unwrap().baseline(), Some(&current));
        let Content::Editable(values) = &current.content else {
            unreachable!()
        };
        assert_eq!(session.picture().unwrap().draft(), Some(values));
        let (session, _, opaque_current) = ready(true);
        assert!(plan_colors(&session, &opaque_current).is_err());
    }
}
