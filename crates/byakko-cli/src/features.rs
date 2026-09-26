//! Typed file intents over cached feature observations and shared Session transitions.
use super::execute;
use byakko_core::{
    editor::Status,
    model::{lighting, picture, settings},
    session::{Connection, Outcome, Session},
    validation,
};
use byakko_devices::Executor;
use std::time::Duration;

fn connect(session: &mut Session, executor: &Executor) -> Result<(), String> {
    if matches!(session.connection(), Connection::Disconnected) {
        executor.set_generation(session.connect()?);
    }
    Ok(())
}
pub fn read_lighting(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<lighting::Snapshot, String> {
    connect(session, executor)?;
    let command = session.read_lighting()?;
    match execute(session, executor, command, Some(timeout))? {
        Outcome::LightingLoaded => Ok(session.lighting().unwrap().baseline().unwrap().clone()),
        outcome => Err(format!("Lighting read did not load: {outcome:?}")),
    }
}
pub fn read_picture(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<picture::Snapshot, String> {
    connect(session, executor)?;
    let command = session.read_picture()?;
    match execute(session, executor, command, Some(timeout))? {
        Outcome::PictureLoaded => Ok(session.picture().unwrap().baseline().unwrap().clone()),
        outcome => Err(format!("Picture read did not load: {outcome:?}")),
    }
}
pub fn read_settings(
    session: &mut Session,
    executor: &Executor,
    timeout: Duration,
) -> Result<settings::Snapshot, String> {
    connect(session, executor)?;
    let command = session.read_settings()?;
    match execute(session, executor, command, Some(timeout))? {
        Outcome::SettingsLoaded => Ok(session.settings().unwrap().baseline().unwrap().clone()),
        outcome => Err(format!("Settings read did not load: {outcome:?}")),
    }
}
pub fn plan_lighting(
    session: &Session,
    target: &lighting::Snapshot,
) -> Result<lighting::Setting, String> {
    let editor = session.lighting().ok_or("Lighting is not supported")?;
    if editor.status() != &Status::Ready || editor.dirty() {
        return Err(
            "Read lighting and save or revert existing edits before planning a file".into(),
        );
    }
    let baseline = editor.baseline().ok_or("No lighting baseline")?;
    if target.revision != baseline.revision || target.picture_context != baseline.picture_context {
        return Err("Lighting file revision or selector differs from the loaded device".into());
    }
    if matches!(baseline.content, lighting::Content::Opaque { .. }) {
        return Err("Opaque lighting cannot be converted".into());
    }
    validation::lighting::validate_snapshot(editor.capabilities(), target)?;
    let lighting::Content::Editable(setting) = &target.content else {
        return Err("Lighting file is not editable".into());
    };
    if target.content == baseline.content {
        return Err("Lighting file contains no changes".into());
    }
    Ok(setting.clone())
}
pub fn apply_lighting(
    session: &mut Session,
    executor: &Executor,
    target: &lighting::Snapshot,
) -> Result<lighting::Snapshot, String> {
    let setting = plan_lighting(session, target)?;
    session.stage_lighting(setting)?;
    let command = session.save_lighting()?;
    match execute(session, executor, command, None)? {
        Outcome::LightingSaved => Ok(session.lighting().unwrap().baseline().unwrap().clone()),
        outcome => Err(format!("Lighting save did not complete: {outcome:?}")),
    }
}
pub fn plan_picture(
    session: &Session,
    target: &picture::Snapshot,
) -> Result<Vec<picture::Edit>, String> {
    let editor = session.picture().ok_or("Picture is not supported")?;
    if editor.status() != &Status::Ready || editor.dirty() {
        return Err("Read picture and save or revert existing edits before planning a file".into());
    }
    let baseline = editor.baseline().ok_or("No picture baseline")?;
    if target.revision != baseline.revision || target.context_revision != baseline.context_revision
    {
        return Err("Picture file revision or selector differs from the loaded device".into());
    }
    validation::picture::validate_snapshot(editor.capabilities(), target)?;
    let (picture::Content::Editable(original), picture::Content::Editable(colors)) =
        (&baseline.content, &target.content)
    else {
        return Err("Opaque picture cannot be converted".into());
    };
    let changes: Vec<_> = colors
        .iter()
        .filter(|(key, color)| original.get(*key) != Some(*color))
        .map(|(key, color)| picture::Edit::Color {
            key: key.clone(),
            color: *color,
        })
        .collect();
    if changes.is_empty() {
        return Err("Picture file contains no changes".into());
    }
    Ok(changes)
}
pub fn apply_picture(
    session: &mut Session,
    executor: &Executor,
    target: &picture::Snapshot,
) -> Result<picture::Snapshot, String> {
    plan_picture(session, target)?;
    session.stage_picture_snapshot(target)?;
    let command = session.save_picture()?;
    match execute(session, executor, command, None)? {
        Outcome::PictureSaved => Ok(session.picture().unwrap().baseline().unwrap().clone()),
        outcome => Err(format!("Picture save did not complete: {outcome:?}")),
    }
}
pub fn plan_settings(
    session: &Session,
    target: &settings::Snapshot,
) -> Result<settings::Edit, String> {
    let editor = session.settings().ok_or("Settings are not supported")?;
    if editor.status() != &Status::Ready || editor.dirty() {
        return Err(
            "Read settings and save or revert existing edits before planning a file".into(),
        );
    }
    let baseline = editor.baseline().ok_or("No settings baseline")?;
    if target.revision != baseline.revision {
        return Err("Settings file revision differs from the loaded device".into());
    }
    validation::settings::validate_snapshot(editor.capabilities(), target)?;
    let (settings::Content::Editable(original), settings::Content::Editable(values)) =
        (&baseline.content, &target.content)
    else {
        return Err("Opaque settings cannot be converted".into());
    };
    let changes: Vec<_> = values
        .iter()
        .filter(|(id, value)| original.get(*id) != Some(*value))
        .map(|(id, value)| settings::Edit {
            id: id.clone(),
            value: value.clone(),
        })
        .collect();
    let [edit] = changes
        .try_into()
        .map_err(|_| "A settings file must change exactly one scalar field")?;
    Ok(edit)
}
pub fn apply_settings(
    session: &mut Session,
    executor: &Executor,
    target: &settings::Snapshot,
) -> Result<settings::Snapshot, String> {
    let edit = plan_settings(session, target)?;
    session.edit_settings(edit)?;
    let command = session.save_settings()?;
    match execute(session, executor, command, None)? {
        Outcome::SettingsSaved => Ok(session.settings().unwrap().baseline().unwrap().clone()),
        outcome => Err(format!("Settings save did not verify: {outcome:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{
        contract::ApplyFailure,
        model::keymap::{Change, State},
    };
    use byakko_devices::{Device, memory::MemoryDevice};
    use std::{
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    struct Counted {
        device: MemoryDevice,
        reads: Arc<[AtomicUsize; 3]>,
    }
    impl Device for Counted {
        fn read(&mut self) -> Result<State, String> {
            self.device.read()
        }
        fn apply(
            &mut self,
            expected: &State,
            changes: &[Change],
            backup: &Path,
        ) -> Result<State, ApplyFailure> {
            self.device.apply(expected, changes, backup)
        }
        fn read_lighting(&mut self) -> Result<lighting::Snapshot, String> {
            self.reads[0].fetch_add(1, Ordering::SeqCst);
            self.device.read_lighting()
        }
        fn apply_lighting(
            &mut self,
            expected: &lighting::Snapshot,
            desired: &lighting::Setting,
            backup: &Path,
        ) -> Result<lighting::Snapshot, ApplyFailure> {
            self.device.apply_lighting(expected, desired, backup)
        }
        fn read_picture(&mut self) -> Result<picture::Snapshot, String> {
            self.reads[1].fetch_add(1, Ordering::SeqCst);
            self.device.read_picture()
        }
        fn apply_picture(
            &mut self,
            expected: &picture::Snapshot,
            desired: &std::collections::BTreeMap<String, [u8; 3]>,
            backup: &Path,
        ) -> Result<picture::Snapshot, ApplyFailure> {
            self.device.apply_picture(expected, desired, backup)
        }
        fn read_settings(&mut self) -> Result<settings::Snapshot, String> {
            self.reads[2].fetch_add(1, Ordering::SeqCst);
            self.device.read_settings()
        }
        fn apply_setting(
            &mut self,
            expected: &settings::Snapshot,
            desired: &settings::Edit,
            backup: &Path,
        ) -> Result<settings::Snapshot, ApplyFailure> {
            self.device.apply_setting(expected, desired, backup)
        }
    }
    fn fixture() -> (Session, Executor, Arc<[AtomicUsize; 3]>) {
        let device = byakko_devices::memory::demo().unwrap();
        let session = device.session().unwrap();
        let reads = Arc::new(std::array::from_fn(|_| AtomicUsize::new(0)));
        let worker = Executor::spawn(
            Counted {
                device,
                reads: Arc::clone(&reads),
            },
            Default::default(),
        )
        .unwrap();
        (session, worker, reads)
    }
    #[test]
    fn feature_files_use_one_cached_read_and_native_completion_evidence() {
        let (mut session, worker, reads) = fixture();
        let timeout = Duration::from_secs(2);
        let mut light = read_lighting(&mut session, &worker, timeout).unwrap();
        let lighting::Content::Editable(setting) = &mut light.content else {
            panic!("editable")
        };
        setting.brightness = Some(4);
        assert!(plan_lighting(&session, &light).is_ok());
        let saved = apply_lighting(&mut session, &worker, &light).unwrap();
        assert_eq!(saved.evidence, lighting::Evidence::TransportAccepted);
        assert!(apply_lighting(&mut session, &worker, &light).is_err());
        let mut pic = read_picture(&mut session, &worker, timeout).unwrap();
        let picture::Content::Editable(colors) = &mut pic.content else {
            panic!("editable")
        };
        colors.insert("Alpha".into(), [1, 2, 3]);
        let mut wrong = pic.clone();
        wrong.context_revision = b"wrong".to_vec();
        assert!(apply_picture(&mut session, &worker, &wrong).is_err());
        assert!(!session.picture().unwrap().dirty());
        let saved = apply_picture(&mut session, &worker, &pic).unwrap();
        assert_eq!(saved.evidence, picture::Evidence::TransportAccepted);
        let mut scalar = read_settings(&mut session, &worker, timeout).unwrap();
        let settings::Content::Editable(values) = &mut scalar.content else {
            panic!("editable")
        };
        values.insert("sleep".into(), settings::Value::Number(0));
        let mut two = scalar.clone();
        let settings::Content::Editable(values) = &mut two.content else {
            panic!("editable")
        };
        values.insert("indicator".into(), settings::Value::Toggle(false));
        assert!(apply_settings(&mut session, &worker, &two).is_err());
        assert!(!session.settings().unwrap().dirty());
        let saved = apply_settings(&mut session, &worker, &scalar).unwrap();
        assert_eq!(saved.content, scalar.content);
        assert_eq!(
            reads
                .iter()
                .map(|count| count.load(Ordering::SeqCst))
                .collect::<Vec<_>>(),
            vec![1, 1, 1]
        );
    }
    #[test]
    fn files_do_not_mix_existing_drafts_or_bypass_selector_invalidation() {
        let (mut session, worker, _) = fixture();
        let timeout = Duration::from_secs(2);
        let mut light = read_lighting(&mut session, &worker, timeout).unwrap();
        let mut pic = read_picture(&mut session, &worker, timeout).unwrap();
        session
            .edit_lighting(lighting::Edit::Brightness(4))
            .unwrap();
        assert!(apply_lighting(&mut session, &worker, &light).is_err());
        session.revert_lighting().unwrap();
        let lighting::Content::Editable(setting) = &mut light.content else {
            panic!("editable")
        };
        setting.effect = "picture".into();
        setting.color = None;
        apply_lighting(&mut session, &worker, &light).unwrap();
        assert!(matches!(
            session.picture().unwrap().status(),
            Status::Unverified { .. }
        ));
        let picture::Content::Editable(colors) = &mut pic.content else {
            panic!("editable")
        };
        colors.insert("Alpha".into(), [1, 2, 3]);
        assert!(apply_picture(&mut session, &worker, &pic).is_err());
    }
}
