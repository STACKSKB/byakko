//! Synchronous client of the same pure session used by Iced.
use byakko_core::{
    Change, State,
    contract::Command,
    keymap::{Status, validate_edit},
    session::{Connection, Outcome, Session},
    validate_state,
};
use byakko_devices::Executor;
use std::time::{Duration, Instant};

/// Shared file boundary for snapshot commands; no metadata/read double pass.
pub fn read_json<T: serde::de::DeserializeOwned>(
    reader: impl std::io::Read,
    limit: u64,
) -> Result<T, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader
        .take(limit.checked_add(1).ok_or("Invalid JSON limit")?)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("Snapshot exceeds its JSON size limit".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

/// Only reads use a timeout. Once a save begins, wait for its recovery outcome.
fn execute(
    session: &mut Session,
    executor: &Executor,
    command: Command,
    timeout: Option<Duration>,
) -> Result<Outcome, String> {
    if let Err(completion) = executor.try_submit(command) {
        return Ok(session.accept(*completion));
    }
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    loop {
        let remaining = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        match executor.receive(remaining) {
            Ok(completion) => match session.accept(completion) {
                Outcome::Ignored => continue,
                outcome => return Ok(outcome),
            },
            Err(error) => {
                executor.set_generation(0);
                session.disconnect();
                return Err(format!("Device operation did not complete: {error}"));
            }
        }
    }
}

pub fn read_keymap(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<State, String> {
    if matches!(session.connection(), Connection::Disconnected) {
        executor.set_generation(session.connect()?);
    }
    let command = session.read()?;
    match execute(session, executor, command, Some(timeout))? {
        Outcome::Loaded => Ok(session
            .keymap()
            .baseline()
            .expect("loaded outcome owns a baseline")
            .clone()),
        outcome => Err(format!("Keymap read did not load: {outcome:?}")),
    }
}

/// A file retains its original revision while changing only advertised bindings.
/// Validate the whole edit list before staging any of it.
pub fn plan_keymap(session: &Session, target: &State) -> Result<Vec<Change>, String> {
    if session.keymap().status() != &Status::Ready {
        return Err("Read the keymap before planning changes".into());
    }
    let current = session
        .keymap()
        .baseline()
        .expect("ready editor has a baseline");
    if target.revision != current.revision {
        return Err("Keymap file revision differs from the loaded keyboard".into());
    }
    validate_state(session.descriptor(), target)?;
    let changes: Vec<_> = target
        .bindings
        .iter()
        .flat_map(|(layer, values)| {
            values
                .iter()
                .filter(|(key, action)| current.bindings[layer].get(*key) != Some(*action))
                .map(move |(key, action)| Change {
                    layer: layer.clone(),
                    key: key.clone(),
                    action: action.clone(),
                })
        })
        .collect();
    for change in &changes {
        validate_edit(session.descriptor(), change)?;
    }
    Ok(changes)
}

pub fn apply_keymap(
    session: &mut Session,
    executor: &Executor,
    target: &State,
) -> Result<State, String> {
    if session.keymap().dirty() {
        return Err("Save or revert staged assignments before applying a keymap file".into());
    }
    let changes = plan_keymap(session, target)?;
    if changes.is_empty() {
        return Err("Keymap file contains no changes".into());
    }
    for change in changes {
        session.edit(change)?;
    }
    let command = session.save()?;
    match execute(session, executor, command, None)? {
        Outcome::Saved => Ok(session
            .keymap()
            .baseline()
            .expect("saved outcome owns a baseline")
            .clone()),
        outcome => Err(format!("Keymap save did not verify: {outcome:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::Action;

    #[test]
    fn file_planning_is_atomic_and_apply_uses_shared_core() {
        let device = byakko_devices::memory::demo().unwrap();
        let mut session = Session::new(device.descriptor().clone()).unwrap();
        let worker = Executor::spawn(device, Default::default()).unwrap();
        let original = read_keymap(&mut session, &worker, Duration::from_secs(1)).unwrap();
        let mut target = original.clone();
        session
            .edit(Change {
                layer: "Navigation".into(),
                key: "Beta".into(),
                action: Action::Key(5),
            })
            .unwrap();
        assert!(apply_keymap(&mut session, &worker, &target).is_err());
        assert_eq!(
            session.keymap().draft().unwrap()["Navigation"]["Beta"],
            Action::Key(5)
        );
        session.revert().unwrap();
        target
            .bindings
            .get_mut("Typing")
            .unwrap()
            .insert("Alpha".into(), Action::Key(5));
        target
            .bindings
            .get_mut("Typing")
            .unwrap()
            .insert("Fixed".into(), Action::Disabled);
        assert!(apply_keymap(&mut session, &worker, &target).is_err());
        assert!(
            !session.keymap().dirty(),
            "a rejected file must not stage its valid prefix"
        );
        target
            .bindings
            .get_mut("Typing")
            .unwrap()
            .insert("Fixed".into(), Action::Key(4));
        let actual = apply_keymap(&mut session, &worker, &target).unwrap();
        assert_eq!(actual.bindings, target.bindings);
        assert_ne!(actual.revision, original.revision);
        assert!(
            plan_keymap(&session, &target).is_err(),
            "old revision cannot overwrite a new save"
        );
    }

    #[test]
    fn bounded_json_accepts_limit_and_rejects_extra_bytes() {
        assert_eq!(read_json::<String>(b"\"ok\"".as_slice(), 4).unwrap(), "ok");
        assert!(read_json::<String>(b"\"ok\" ".as_slice(), 4).is_err());
    }
}
