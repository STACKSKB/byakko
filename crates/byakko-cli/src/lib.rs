//! Synchronous CLI adapter for the same owned session commands used by Iced.
use byakko_core::{
    State,
    lighting::{Snapshot as LightingSnapshot, editor::Status as LightingStatus},
    macros::{Snapshot as MacroSnapshot, editor::Status as MacroStatus},
    picture::{Snapshot as PictureSnapshot, editor::Status as PictureStatus},
    session::{Acceptance, Command, Session, Status},
    settings::{Snapshot as SettingsSnapshot, editor::Status as SettingsStatus},
};
use byakko_devices::Executor;
use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

pub fn read_keymap(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<State, String> {
    let generation = session.connect()?;
    executor.set_generation(generation);
    let command = session.request_read()?;
    submit_and_wait(session, executor, command, timeout)?;
    match session.status() {
        Status::Ready => session
            .baseline()
            .cloned()
            .ok_or("Verified read has no baseline".into()),
        status => Err(format!("Device read failed: {status:?}")),
    }
}

/// Read one advertised macro slot without interpreting its backend-owned snapshot.
pub fn read_macro(
    session: &mut Session,
    executor: &Executor,
    slot: &str,
    timeout: Duration,
) -> Result<MacroSnapshot, String> {
    let editor = session.macros().ok_or("Device does not support macros")?;
    if !editor
        .capabilities()
        .slots
        .iter()
        .any(|choice| choice.id == slot)
    {
        return Err(format!("Unknown macro slot: {slot}"));
    }
    session.select_macro(slot)?;
    if *session.status() == Status::Disconnected {
        executor.set_generation(session.connect()?);
    }
    let command = session.request_macro_read()?;
    submit_and_wait(session, executor, command, timeout)?;
    let editor = session.macros().ok_or("Device does not support macros")?;
    match editor.status() {
        MacroStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified macro read has no baseline".into()),
        status => Err(format!("Macro read failed: {status:?}")),
    }
}

pub fn read_colors(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<PictureSnapshot, String> {
    if *session.status() != Status::Ready {
        return Err("Read the keymap before reading colors".into());
    }
    let command = session.request_picture_read()?;
    submit_and_wait(session, executor, command, timeout)?;
    let editor = session.picture().ok_or("No per-key color capability")?;
    match editor.status() {
        PictureStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified color read has no baseline".into()),
        status => Err(format!("Color read failed: {status:?}")),
    }
}

pub fn read_lighting(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<LightingSnapshot, String> {
    if *session.status() != Status::Ready {
        return Err("Read the keymap before reading lighting".into());
    }
    let command = session.request_lighting_read()?;
    submit_and_wait(session, executor, command, timeout)?;
    let editor = session.lighting().ok_or("No lighting capability")?;
    match editor.status() {
        LightingStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified lighting read has no baseline".into()),
        status => Err(format!("Lighting read failed: {status:?}")),
    }
}

pub fn read_settings(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<SettingsSnapshot, String> {
    if *session.status() != Status::Ready {
        return Err("Read the keymap before reading settings".into());
    }
    let command = session.request_settings_read()?;
    submit_and_wait(session, executor, command, timeout)?;
    let editor = session.settings().ok_or("No settings capability")?;
    match editor.status() {
        SettingsStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified settings read has no baseline".into()),
        status => Err(format!("Settings read failed: {status:?}")),
    }
}

fn submit_and_wait(
    session: &mut Session,
    executor: &Executor,
    command: Command,
    timeout: Duration,
) -> Result<(), String> {
    if let Err(rejected) = executor.try_submit(command) {
        session.accept(*rejected);
        return Err(format!("Device request rejected: {:?}", session.status()));
    }
    let deadline = Instant::now() + timeout;
    loop {
        match executor.try_receive() {
            Ok(completion) => {
                if session.accept(completion) == Acceptance::IgnoredStale {
                    return Err("Stale device completion".into());
                }
                return Ok(());
            }
            Err(TryRecvError::Disconnected) => return Err("Device executor stopped".into()),
            Err(TryRecvError::Empty) if Instant::now() >= deadline => {
                return Err("Device read timed out; no configuration was written".into());
            }
            Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{
        Action, ActionChoice, Descriptor, Layer, PhysicalKey, lighting, macros, picture, settings,
    };
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf};

    #[test]
    fn cli_reads_keymap_colors_lighting_and_settings_through_shared_executor() {
        let descriptor = Descriptor {
            backend_id: "synthetic".into(),
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
            actions: vec![ActionChoice {
                label: "A".into(),
                action: Action::Key(4),
            }],
        };
        let state = State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([("one".into(), Action::Key(4))]),
            )]),
        };
        let capabilities = picture::Capabilities {
            backend_id: "synthetic".into(),
            keys: vec!["one".into()],
        };
        let colors = picture::Snapshot {
            backend_id: "synthetic".into(),
            revision: vec![2],
            content: picture::Content::Editable(BTreeMap::from([("one".into(), [12, 34, 56])])),
        };
        let lighting_capabilities = lighting::Capabilities {
            backend_id: "synthetic".into(),
            effects: vec![lighting::Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: Some(0..=10),
                speed: None,
                options: vec![],
                color: Some(lighting::ColorCapability::Fixed),
            }],
            host_modes: vec![],
        };
        let lighting = lighting::Snapshot {
            backend_id: "synthetic".into(),
            revision: vec![3],
            content: lighting::Content::Editable(lighting::Setting {
                effect: "steady".into(),
                brightness: Some(5),
                speed: None,
                option: None,
                color: Some(lighting::Color::Rgb([12, 34, 56])),
            }),
        };
        let settings_capabilities = settings::Capabilities {
            backend_id: "synthetic".into(),
            fields: vec![settings::Field {
                id: "sleep".into(),
                label: "Sleep".into(),
                kind: settings::Kind::Toggle,
            }],
        };
        let settings = settings::Snapshot {
            backend_id: "synthetic".into(),
            revision: vec![4],
            content: settings::Content::Editable(BTreeMap::from([(
                "sleep".into(),
                settings::Value::Toggle(true),
            )])),
        };
        let device = MemoryDevice::new(descriptor.clone(), state.clone())
            .unwrap()
            .with_picture(capabilities.clone(), colors.clone())
            .unwrap()
            .with_lighting(lighting_capabilities.clone(), lighting.clone())
            .unwrap()
            .with_settings(settings_capabilities.clone(), settings.clone())
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor)
            .unwrap()
            .with_picture(capabilities)
            .unwrap()
            .with_lighting(lighting_capabilities)
            .unwrap()
            .with_settings(settings_capabilities)
            .unwrap();
        let observed = read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert_eq!(observed, state);
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(
            read_colors(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            colors
        );
        assert_eq!(
            read_lighting(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            lighting
        );
        assert_eq!(
            read_settings(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            settings
        );
    }

    #[test]
    fn cli_reads_macro_snapshot_and_preserves_opaque_revision_bytes() {
        let descriptor = Descriptor {
            backend_id: "synthetic".into(),
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
            actions: vec![ActionChoice {
                label: "A".into(),
                action: Action::Key(4),
            }],
        };
        let state = State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([("one".into(), Action::Key(4))]),
            )]),
        };
        let capabilities = macros::Capabilities {
            byte_budget: None,
            backend_id: "synthetic".into(),
            slots: vec![macros::Choice {
                id: "custom-slot".into(),
                label: "Custom".into(),
            }],
            repeat_counts: 0..=10,
            editable_repeat_counts: 0..=10,
            delays_ms: 0..=100,
            keys: None,
            buttons: vec![],
            movement: None,
            backend_actions: vec![],
            bindings: vec![],
        };
        let snapshot = macros::Snapshot {
            backend_id: "synthetic".into(),
            slot: "custom-slot".into(),
            revision: vec![0, 255, 3, 128],
            content: macros::Content::Opaque {
                reason: "Unknown encoding".into(),
            },
        };
        let device = MemoryDevice::new(descriptor.clone(), state)
            .unwrap()
            .with_macros(capabilities.clone(), vec![snapshot.clone()])
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor)
            .unwrap()
            .with_macros(capabilities)
            .unwrap();

        assert!(read_macro(&mut session, &executor, "missing", Duration::from_secs(1)).is_err());
        assert_eq!(
            read_macro(
                &mut session,
                &executor,
                "custom-slot",
                Duration::from_secs(1)
            )
            .unwrap(),
            snapshot
        );
    }
}
