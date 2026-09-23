//! Synchronous CLI adapter for the same owned session commands used by Iced.
use byakko_core::{
    State,
    session::{Session, Status},
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
    if let Err(rejected) = executor.try_submit(command) {
        session.accept(*rejected);
        return Err(format!("Device request rejected: {:?}", session.status()));
    }
    let deadline = Instant::now() + timeout;
    loop {
        match executor.try_receive() {
            Ok(completion) => {
                session.accept(completion);
                return match session.status() {
                    Status::Ready => session
                        .baseline()
                        .cloned()
                        .ok_or("Verified read has no baseline".into()),
                    status => Err(format!("Device read failed: {status:?}")),
                };
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
    use byakko_core::{Action, ActionChoice, Descriptor, Layer, PhysicalKey};
    use byakko_devices::memory::MemoryDevice;
    use std::{collections::BTreeMap, path::PathBuf};

    #[test]
    fn cli_read_uses_session_command_and_shared_executor() {
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
        let device = MemoryDevice::new(descriptor.clone(), state.clone()).unwrap();
        let executor = Executor::spawn(device, PathBuf::new()).unwrap();
        let mut session = Session::new(descriptor).unwrap();
        let observed = read_keymap(&mut session, &executor, Duration::from_secs(1)).unwrap();
        assert_eq!(observed, state);
        assert_eq!(session.status(), &Status::Ready);
    }
}
