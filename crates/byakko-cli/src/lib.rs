//! Synchronous CLI adapter for the same owned session commands used by Iced.
mod keymap;
mod lighting;
mod macro_snapshot;
mod picture;
mod settings;
use byakko_core::{
    State,
    archive::{ArchiveState, NativeArchive, Review},
    lighting::{Snapshot as LightingSnapshot, editor::Status as LightingStatus},
    macros::{Snapshot as MacroSnapshot, editor::Status as MacroStatus},
    picture::{Snapshot as PictureSnapshot, editor::Status as PictureStatus},
    session::{Acceptance, Command, Session, Status},
    settings::{Snapshot as SettingsSnapshot, editor::Status as SettingsStatus},
};
use byakko_devices::Executor;
pub use keymap::{apply_keymap, plan_keymap};
pub use lighting::{apply_lighting, plan_lighting};
pub use macro_snapshot::{apply_macro, plan_macro};
pub use picture::{apply_colors, plan_colors};
pub use settings::{apply_settings, plan_settings};
use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MacroLibrary {
    pub capacity: usize,
    pub configured: Vec<byakko_core::macros::Choice>,
    pub next_free: Option<String>,
}

pub fn read_keymap(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<State, String> {
    let generation = session.connect()?;
    executor.set_generation(generation);
    let command = session.request_read()?;
    submit_and_wait(session, executor, command, Some(timeout))?;
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
    submit_and_wait(session, executor, command, Some(timeout))?;
    let editor = session.macros().ok_or("Device does not support macros")?;
    match editor.status() {
        MacroStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified macro read has no baseline".into()),
        status => Err(format!("Macro read failed: {status:?}")),
    }
}

/// Read the complete advertised macro library through the shared session.
pub fn read_macro_library(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<MacroLibrary, String> {
    if *session.status() != Status::Ready {
        return Err("Read the keymap before listing macros".into());
    }
    let command = session.request_macro_catalog_read()?;
    submit_and_wait(session, executor, command, Some(timeout))?;
    let editor = session.macros().ok_or("Device does not support macros")?;
    if let Some(error) = editor.catalog_error() {
        return Err(format!("Macro library read failed: {error}"));
    }
    let configured = session
        .macro_library_slots()
        .ok_or("Macro library is incomplete")?
        .into_iter()
        .cloned()
        .collect();
    Ok(MacroLibrary {
        capacity: editor.capabilities().slots.len(),
        configured,
        next_free: session.next_free_macro_slot().map(str::to_owned),
    })
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
    submit_and_wait(session, executor, command, Some(timeout))?;
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
    submit_and_wait(session, executor, command, Some(timeout))?;
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
    submit_and_wait(session, executor, command, Some(timeout))?;
    let editor = session.settings().ok_or("No settings capability")?;
    match editor.status() {
        SettingsStatus::Ready => editor
            .baseline()
            .cloned()
            .ok_or("Verified settings read has no baseline".into()),
        status => Err(format!("Settings read failed: {status:?}")),
    }
}

pub fn capture_archive(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<NativeArchive, String> {
    if *session.status() != Status::Ready {
        return Err("Read the keymap before capturing an archive".into());
    }
    let command = session.request_archive_capture()?;
    submit_and_wait(session, executor, command, Some(timeout))?;
    match session.archive() {
        Some(ArchiveState::Captured(snapshot)) => Ok(snapshot.clone()),
        Some(state) => Err(format!("Archive capture failed: {state:?}")),
        None => Err("No native archive capability".into()),
    }
}

pub fn review_archive(
    session: &mut Session,
    executor: &Executor,
    target: NativeArchive,
    timeout: Duration,
) -> Result<Review, String> {
    if *session.status() != Status::Ready {
        return Err("Read the keymap before reviewing an archive".into());
    }
    let command = session.request_archive_review(target)?;
    submit_and_wait(session, executor, command, Some(timeout))?;
    match session.archive() {
        Some(ArchiveState::Ready(review)) => Ok(review.clone()),
        Some(state) => Err(format!("Archive review failed: {state:?}")),
        None => Err("No native archive capability".into()),
    }
}

pub(crate) fn submit_and_wait(
    session: &mut Session,
    executor: &Executor,
    command: Command,
    timeout: Option<Duration>,
) -> Result<(), String> {
    if let Err(rejected) = executor.try_submit(command) {
        session.accept(*rejected);
        return Err(format!("Device request rejected: {:?}", session.status()));
    }
    let deadline = timeout.map(|limit| Instant::now() + limit);
    loop {
        match executor.try_receive() {
            Ok(completion) => {
                if session.accept(completion) == Acceptance::IgnoredStale {
                    return Err("Stale device completion".into());
                }
                return Ok(());
            }
            Err(TryRecvError::Disconnected) => return Err("Device executor stopped".into()),
            Err(TryRecvError::Empty) if deadline.is_some_and(|end| Instant::now() >= end) => {
                return Err("Device operation timed out; its outcome is unknown".into());
            }
            Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{
        Action, ActionChoice, Descriptor, Layer, PhysicalKey, archive, lighting, macros, picture,
        settings,
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
            shortcuts: None,
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
        let archive_caps = archive::ArchiveCapabilities {
            backend_id: "synthetic".into(),
            format_id: "fixture".into(),
            max_bytes: 16,
        };
        let archive = archive::NativeArchive {
            backend_id: "synthetic".into(),
            format_id: "fixture".into(),
            bytes: vec![0, 255, 7],
        };
        let device = MemoryDevice::new(descriptor.clone(), state.clone())
            .unwrap()
            .with_picture(capabilities.clone(), colors.clone())
            .unwrap()
            .with_lighting(lighting_capabilities.clone(), lighting.clone())
            .unwrap()
            .with_settings(settings_capabilities.clone(), settings.clone())
            .unwrap()
            .with_archive(archive_caps.clone(), archive.clone())
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor)
            .unwrap()
            .with_picture(capabilities)
            .unwrap()
            .with_lighting(lighting_capabilities)
            .unwrap()
            .with_settings(settings_capabilities)
            .unwrap()
            .with_archive(archive_caps)
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
        assert_eq!(
            capture_archive(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            archive
        );
        assert!(
            review_archive(
                &mut session,
                &executor,
                archive.clone(),
                Duration::from_secs(1)
            )
            .unwrap()
            .changes
            .is_empty()
        );
        let mut changed = archive;
        changed.bytes[1] = 3;
        let review =
            review_archive(&mut session, &executor, changed, Duration::from_secs(1)).unwrap();
        assert_eq!(review.changes[0].id, "archive");
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
            shortcuts: None,
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
