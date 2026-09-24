//! File-based global lighting through the portable session contract.

use byakko_core::{
    lighting::{self, Content, Setting, Snapshot, editor::Status as LightingStatus},
    session::{Session, Status},
};
use byakko_devices::Executor;

/// Compare one complete editable lighting file with a fresh verified read.
/// The raw revision is an exact before-image token, not editable effect data.
pub fn plan_lighting(session: &Session, target: &Snapshot) -> Result<Option<Setting>, String> {
    if *session.status() != Status::Ready {
        return Err("Read and verify the keymap before planning lighting".into());
    }
    let editor = session
        .lighting()
        .ok_or("Device does not support lighting")?;
    if *editor.status() != LightingStatus::Ready {
        return Err("Read and verify lighting before planning changes".into());
    }
    let current = editor
        .baseline()
        .ok_or("Verified lighting has no baseline")?;
    if current.backend_id != target.backend_id || current.revision != target.revision {
        return Err(
            "Lighting file backend or revision differs from the connected keyboard; read again"
                .into(),
        );
    }
    lighting::validate_snapshot(editor.capabilities(), target)?;
    let Content::Editable(after) = &target.content else {
        return Err("Lighting target must be an onboard effect".into());
    };
    match &current.content {
        Content::Editable(before) => Ok((before != after).then(|| after.clone())),
        Content::HostActive { .. } => Ok(Some(after.clone())),
        Content::Opaque { .. } => Err("Opaque lighting is available only as a raw backup".into()),
    }
}

/// Stage the reviewed effect and await its backed-up transport submission.
pub fn apply_lighting(
    session: &mut Session,
    executor: &Executor,
    target: &Snapshot,
) -> Result<Snapshot, String> {
    let desired = plan_lighting(session, target)?.ok_or("Lighting file contains no changes")?;
    session.stage_lighting(desired)?;
    let command = session.request_lighting_apply()?;
    super::submit_and_wait(session, executor, command, None)?;
    let editor = session
        .lighting()
        .ok_or("Device does not support lighting")?;
    match editor.status() {
        LightingStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Accepted lighting apply has no baseline".into()),
        status => Err(format!("Lighting upload did not complete: {status:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_keymap, read_lighting};
    use byakko_core::{
        Action, Descriptor, Layer, PhysicalKey, State,
        lighting::{Capabilities, Color, ColorCapability, Effect, HostMode, HostSource},
    };
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf, time::Duration};

    fn ready() -> (Session, Executor, Snapshot) {
        ready_with(Content::Editable(Setting {
            effect: "steady".into(),
            brightness: Some(4),
            speed: None,
            option: None,
            color: Some(Color::Rgb([10, 20, 30])),
        }))
    }

    fn ready_with(content: Content) -> (Session, Executor, Snapshot) {
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
            effects: vec![Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: Some(0..=4),
                speed: None,
                options: vec![],
                color: Some(ColorCapability::Fixed),
            }],
            host_modes: vec![HostMode {
                id: "screen".into(),
                label: "Screen".into(),
                source: HostSource::ScreenAverage,
                parameters: None,
            }],
        };
        let snapshot = Snapshot {
            evidence: byakko_core::SnapshotEvidence::Readback,
            backend_id: "memory".into(),
            revision: vec![2],
            content,
        };
        let device = MemoryDevice::new(descriptor.clone(), state)
            .unwrap()
            .with_lighting(capabilities.clone(), snapshot)
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor)
            .unwrap()
            .with_lighting(capabilities)
            .unwrap();
        read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        let current = read_lighting(&mut session, &executor, Duration::from_secs(1)).unwrap();
        (session, executor, current)
    }

    #[test]
    fn known_host_mode_exit_uses_the_same_revision_checked_apply() {
        let (mut session, executor, current) = ready_with(Content::HostActive {
            mode_id: "screen".into(),
        });
        assert!(plan_lighting(&session, &current).is_err());
        let mut target = current.clone();
        target.content = Content::Editable(Setting {
            effect: "steady".into(),
            brightness: Some(4),
            speed: None,
            option: None,
            color: Some(Color::Rgb([10, 20, 30])),
        });
        assert!(plan_lighting(&session, &target).unwrap().is_some());
        let applied = apply_lighting(&mut session, &executor, &target).unwrap();
        assert_eq!(applied.content, target.content);
        assert_ne!(applied.revision, current.revision);
        assert!(plan_lighting(&session, &target).is_err());
    }

    #[test]
    fn plan_apply_and_reread_use_shared_transaction() {
        let (mut session, executor, current) = ready();
        assert_eq!(plan_lighting(&session, &current).unwrap(), None);
        assert!(apply_lighting(&mut session, &executor, &current).is_err());
        let mut target = current.clone();
        let Content::Editable(setting) = &mut target.content else {
            panic!("expected editable lighting")
        };
        setting.brightness = Some(2);
        let desired = setting.clone();
        assert_eq!(plan_lighting(&session, &target).unwrap(), Some(desired));
        let applied = apply_lighting(&mut session, &executor, &target).unwrap();
        assert_eq!(applied.content, target.content);
        assert_ne!(applied.revision, current.revision);
        read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert_eq!(
            read_lighting(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            applied
        );
        assert!(plan_lighting(&session, &target).is_err());
    }

    #[test]
    fn malformed_stale_and_opaque_files_do_not_stage() {
        let (mut session, executor, current) = ready();
        let mut stale = current.clone();
        stale.revision.push(0);
        assert!(plan_lighting(&session, &stale).is_err());
        let mut foreign = current.clone();
        foreign.backend_id = "other".into();
        assert!(plan_lighting(&session, &foreign).is_err());
        let mut invalid = current.clone();
        let Content::Editable(setting) = &mut invalid.content else {
            panic!("expected editable lighting")
        };
        setting.brightness = Some(5);
        assert!(plan_lighting(&session, &invalid).is_err());
        let mut opaque = current.clone();
        opaque.content = Content::Opaque {
            reason: "unknown".into(),
        };
        assert!(plan_lighting(&session, &opaque).is_err());
        assert!(apply_lighting(&mut session, &executor, &invalid).is_err());
        let editor = session.lighting().unwrap();
        assert_eq!(editor.status(), &LightingStatus::Ready);
        assert_eq!(editor.baseline(), Some(&current));
        assert_eq!(
            editor.draft(),
            match &current.content {
                Content::Editable(setting) => Some(setting),
                _ => None,
            }
        );
    }
}
