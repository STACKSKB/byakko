//! Synchronous client of the same pure session used by Iced.
mod features;
use byakko_core::{
    contract::Command,
    editor::Status,
    model::keymap::{Change, State},
    session::{Connection, Outcome, Session},
    validation::keymap::{validate_edit, validate_state},
};
use byakko_devices::Executor;
pub use features::{
    apply_lighting, apply_picture, apply_settings, plan_lighting, plan_picture, plan_settings,
    read_lighting, read_picture, read_settings,
};
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
    let mut ready = executor
        .try_submit(command)
        .err()
        .map(|completion| *completion);
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    loop {
        let remaining = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        match ready.take().map_or_else(|| executor.receive(remaining), Ok) {
            Ok(completion) => match session.accept(completion) {
                Outcome::Ignored | Outcome::CatalogLoaded | Outcome::CatalogFailed(_) => continue,
                Outcome::Continue(command) => {
                    ready = executor
                        .try_submit(command)
                        .err()
                        .map(|completion| *completion)
                }
                outcome => return Ok(outcome),
            },
            Err(error) => {
                executor.set_generation(0);
                session.disconnect()?;
                return Err(format!("Device operation did not complete: {error}"));
            }
        }
    }
}

pub fn read_macro(
    session: &mut Session,
    executor: &Executor,
    slot: &str,
    timeout: Duration,
) -> Result<byakko_core::model::macros::Snapshot, String> {
    if matches!(session.connection(), Connection::Disconnected) {
        executor.set_generation(session.connect()?);
    }
    session.select_macro(slot)?;
    let command = session.read_macro()?;
    match execute(session, executor, command, Some(timeout))? {
        Outcome::MacroLoaded => Ok(session
            .macros()
            .and_then(|editor| editor.baseline())
            .expect("loaded macro owns a baseline")
            .clone()),
        outcome => Err(format!("Macro read did not load: {outcome:?}")),
    }
}

/// Discovery is itself the foreground task for this CLI command.
pub fn list_macros(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<(), String> {
    if matches!(session.connection(), Connection::Disconnected) {
        executor.set_generation(session.connect()?);
    }
    let command = session.request_macro_catalog()?;
    if let Err(completion) = executor.try_submit(command) {
        return Err(format!(
            "Macro discovery could not start: {:?}",
            session.accept(*completion)
        ));
    }
    let deadline = Instant::now() + timeout;
    loop {
        let completion =
            match executor.receive(Some(deadline.saturating_duration_since(Instant::now()))) {
                Ok(completion) => completion,
                Err(error) => {
                    executor.set_generation(0);
                    session.disconnect()?;
                    return Err(format!("Macro discovery did not complete: {error}"));
                }
            };
        match session.accept(completion) {
            Outcome::Ignored => continue,
            Outcome::CatalogLoaded => return Ok(()),
            outcome => return Err(format!("Macro discovery did not load: {outcome:?}")),
        }
    }
}

pub fn apply_macro(
    session: &mut Session,
    executor: &Executor,
    target: &byakko_core::model::macros::Snapshot,
) -> Result<byakko_core::model::macros::Snapshot, String> {
    if session.macros().is_some_and(|editor| editor.dirty()) {
        return Err("Save or revert staged macro edits before applying a macro file".into());
    }
    session.stage_macro_snapshot(target)?;
    let command = session.save_macro()?;
    match execute(session, executor, command, None)? {
        Outcome::MacroSaved => Ok(session
            .macros()
            .and_then(|editor| editor.baseline())
            .expect("saved macro owns a baseline")
            .clone()),
        outcome => Err(format!("Macro save did not verify: {outcome:?}")),
    }
}

pub fn assign_macro(
    session: &mut Session,
    executor: &Executor,
    layer: &str,
    key: &str,
    binding: &str,
) -> Result<(), String> {
    let command = session.save_and_assign_macro(layer, key, binding)?;
    match execute(session, executor, command, None)? {
        Outcome::AssignmentSucceeded { .. } => Ok(()),
        outcome => Err(format!("Macro assignment did not complete: {outcome:?}")),
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
    use byakko_core::model::keymap::Action;

    #[test]
    fn macro_file_and_assignment_share_session_transitions() {
        use byakko_core::model::macros::{Content, Edit};
        let device = byakko_devices::memory::demo().unwrap();
        let mut session = Session::new(device.descriptor().clone())
            .unwrap()
            .with_macros(device.macro_capabilities().unwrap().clone())
            .unwrap();
        let worker = Executor::spawn(device, Default::default()).unwrap();
        let timeout = Duration::from_secs(2);
        list_macros(&mut session, &worker, timeout).unwrap();
        let mut target = read_macro(&mut session, &worker, "Greeting", timeout).unwrap();
        let Content::Editable(program) = &mut target.content else {
            panic!("demo macro is editable")
        };
        program.repeat_count = 2;
        let saved = apply_macro(&mut session, &worker, &target).unwrap();
        assert_eq!(saved.content, target.content);
        assert!(
            apply_macro(&mut session, &worker, &target).is_err(),
            "stale file is rejected"
        );
        read_keymap(&mut session, &worker, timeout).unwrap();
        session.edit_macro(Edit::Repeat(3)).unwrap();
        assign_macro(&mut session, &worker, "Typing", "Alpha", "play-Greeting").unwrap();
        assert!(!session.macros().unwrap().dirty());
        assert_eq!(session.macros().unwrap().draft().unwrap().repeat_count, 3);
        assert_eq!(
            session.keymap().baseline().unwrap().bindings["Typing"]["Alpha"],
            Action::Named {
                id: "play-Greeting".into()
            }
        );
    }

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
