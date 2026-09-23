//! Synchronous CLI adapter for the same owned session commands used by Iced.
use byakko_core::{
    State,
    picture::{Snapshot as PictureSnapshot, editor::Status as PictureStatus},
    session::{Acceptance, Command, Session, Status},
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
    use byakko_core::{Action, ActionChoice, Descriptor, Layer, PhysicalKey, picture};
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf};

    #[test]
    fn cli_reads_keymap_and_colors_through_shared_executor() {
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
        let device = MemoryDevice::new(descriptor.clone(), state.clone())
            .unwrap()
            .with_picture(capabilities.clone(), colors.clone())
            .unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor)
            .unwrap()
            .with_picture(capabilities)
            .unwrap();
        let observed = read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert_eq!(observed, state);
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(
            read_colors(&mut session, &executor, Duration::from_secs(1)).unwrap(),
            colors
        );
    }
}
