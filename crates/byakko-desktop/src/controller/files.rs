//! One local file job at a time. Core validates imported programs; files never write HID.
use crate::form::files::{Form, Operation};
use byakko_core::{
    model::macros::{Choice, Document},
    session::{Connection, Session},
};
use byakko_devices::storage;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Ticket {
    id: u64,
    connection: Connection,
    operation: Operation,
    slot: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Value {
    Imported(Document),
    Labels(Option<BTreeMap<String, String>>),
    SavedLabels(BTreeMap<String, String>),
    Exported,
}

#[derive(Clone, Debug)]
pub struct Completion {
    pub ticket: Ticket,
    pub result: Result<Value, String>,
}

pub enum Accepted {
    Imported(String),
    Finished(String),
}

pub struct Job {
    ticket: Ticket,
    work: Box<dyn FnOnce() -> Result<Value, String> + Send>,
}
impl Job {
    pub fn run(self) -> Completion {
        Completion {
            ticket: self.ticket,
            result: (self.work)(),
        }
    }
    pub fn task(self) -> iced::Task<Completion> {
        let ticket = self.ticket.clone();
        let (send, receive) = iced::futures::channel::oneshot::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("byakko-file".into())
            .spawn(move || {
                let _ = send.send(self.run());
            })
        {
            return iced::Task::done(Completion {
                ticket,
                result: Err(error.to_string()),
            });
        }
        iced::Task::perform(
            async move {
                receive.await.unwrap_or_else(|_| Completion { ticket, result: Err("File worker stopped without a result. Check the destination before retrying an export.".into()) })
            },
            |completion| completion,
        )
    }
}

pub struct Files {
    pub form: Form,
    pending: Option<Ticket>,
    next: u64,
    labels_directory: Option<PathBuf>,
    labels_loaded: bool,
}

impl Files {
    pub fn new(session: &Session, data_directory: Option<PathBuf>) -> Self {
        let names: BTreeMap<_, _> = session
            .macros()
            .into_iter()
            .flat_map(|editor| editor.capabilities().slots.iter())
            .map(|slot| (slot.id.clone(), slot.label.clone()))
            .collect();
        let labels_directory = data_directory.map(|root| {
            root.join("macro-labels")
                .join(&session.descriptor().backend_id)
        });
        Self {
            form: Form {
                saved_names: names.clone(),
                names,
                ..Form::default()
            },
            pending: None,
            next: 0,
            labels_loaded: labels_directory.is_none(),
            labels_directory,
        }
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn needs_labels(&self) -> bool {
        !self.labels_loaded && !self.form.names.is_empty()
    }
    pub fn can_save_labels(&self) -> bool {
        self.labels_directory.is_some()
    }

    pub fn begin(&mut self, operation: Operation, session: &Session) -> Result<Job, String> {
        if self.busy() || session.busy() || session.recording() {
            return Err("Finish the current operation before file work".into());
        }
        let slot = session.macros().map(|editor| editor.slot().to_owned());
        let work: Box<dyn FnOnce() -> Result<Value, String> + Send> = match operation {
            Operation::ImportMacro => {
                let editor = session.macros().ok_or("Macros are unavailable")?;
                if editor.status() != &byakko_core::editor::Status::Ready
                    || editor.draft().is_none()
                {
                    return Err("Read an editable macro slot before importing".into());
                }
                let path = path(&self.form.macro_path)?;
                Box::new(move || storage::macros::load(&path).map(Value::Imported))
            }
            Operation::ExportMacro => {
                let slot = slot.as_deref().ok_or("Macros are unavailable")?;
                let document = session.export_macro_document(
                    self.form.name(slot).into(),
                    self.form.bindings.get(slot).cloned(),
                )?;
                let path = path(&self.form.macro_path)?;
                Box::new(move || {
                    storage::macros::save_new(&path, &document).map(|()| Value::Exported)
                })
            }
            Operation::LoadLabels | Operation::SaveLabels => {
                let directory = self
                    .labels_directory
                    .clone()
                    .ok_or("Local name storage is unavailable")?;
                let editor = session.macros().ok_or("Macros are unavailable")?;
                let backend = editor.capabilities().backend_id.clone();
                let slots: Vec<Choice> = editor.capabilities().slots.clone();
                if operation == Operation::LoadLabels {
                    Box::new(move || {
                        storage::labels::load_latest(&directory, &backend, &slots)
                            .map(Value::Labels)
                    })
                } else {
                    let names = self.form.names.clone();
                    Box::new(move || {
                        storage::labels::save_new(&directory, &backend, &slots, &names)
                            .map(|_| Value::SavedLabels(names))
                    })
                }
            }
            Operation::ExportArchive => {
                let bytes = session
                    .archive()
                    .and_then(|capture| capture.captured())
                    .ok_or("Capture a configuration before exporting")?
                    .bytes
                    .clone();
                let path = path(&self.form.archive_path)?;
                Box::new(move || storage::write_new(&path, &bytes).map(|()| Value::Exported))
            }
        };
        self.next = self
            .next
            .checked_add(1)
            .ok_or("File operation ID exhausted")?;
        let ticket = Ticket {
            id: self.next,
            connection: session.connection().clone(),
            operation,
            slot,
        };
        self.pending = Some(ticket.clone());
        Ok(Job { ticket, work })
    }

    pub fn accept(
        &mut self,
        completion: Completion,
        session: &mut Session,
    ) -> Option<Result<Accepted, String>> {
        if self.pending.as_ref() != Some(&completion.ticket) {
            return None;
        }
        self.pending = None;
        if completion.ticket.operation == Operation::LoadLabels {
            self.labels_loaded = true;
        }
        if completion.ticket.operation == Operation::ImportMacro
            && completion.ticket.connection != *session.connection()
        {
            return Some(Err(
                "Connection changed during file work; no imported data was staged.".into(),
            ));
        }
        let result = completion.result.and_then(|value| match (completion.ticket.operation, value) {
            (Operation::ImportMacro, Value::Imported(document)) => {
                let slot = completion.ticket.slot.ok_or("Macro import has no selected slot")?;
                if session.macros().is_none_or(|editor| editor.slot() != slot) {
                    return Err("Selected macro changed during import".into());
                }
                let metadata = session.stage_macro_document(&document)?;
                let binding = metadata.binding.filter(|id| {
                    document.backend_id == session.descriptor().backend_id
                        && session.macros().is_some_and(|editor| {
                            editor.capabilities().bindings.iter().any(|binding| {
                                binding.slot == slot && binding.id == *id
                            })
                        })
                });
                let dropped_binding = document.binding.is_some() && binding.is_none();
                self.form.names.insert(slot.clone(), metadata.name);
                match binding {
                    Some(binding) => { self.form.bindings.insert(slot, binding); }
                    None => { self.form.bindings.remove(&slot); }
                }
                Ok(Accepted::Imported(if dropped_binding {
                    "Imported draft. The source binding does not apply to this slot; choose a binding separately."
                } else { "Imported into the selected draft. Review before saving or assigning." }.into()))
            }
            (Operation::LoadLabels, Value::Labels(names)) => {
                if let Some(names) = names {
                    self.form.names = names;
                }
                self.form.saved_names = self.form.names.clone();
                Ok(Accepted::Finished("Local macro names loaded.".into()))
            }
            (Operation::SaveLabels, Value::SavedLabels(names)) => {
                self.form.saved_names = names;
                Ok(Accepted::Finished("Local macro names saved.".into()))
            }
            (Operation::ExportMacro | Operation::ExportArchive, Value::Exported) => Ok(Accepted::Finished("Exported to a new file.".into())),
            _ => Err("File worker returned a result for another operation".into()),
        });
        Some(result)
    }
}

fn path(input: &str) -> Result<PathBuf, String> {
    if input.trim().is_empty() {
        Err("Enter a local file path".into())
    } else {
        Ok(input.trim().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{
        contract::{CompletionPayload, FeatureResult},
        model::macros::{Action, Event, Program},
        session::Outcome,
    };
    use byakko_devices::{Device, memory};
    use std::{
        fs,
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn directory() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "byakko-desktop-files-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        directory
    }
    fn session() -> Session {
        let mut device = memory::demo().unwrap();
        let mut session = device.session().unwrap();
        session.connect().unwrap();
        let command = session.read_macro().unwrap();
        let slot = session.macros().unwrap().slot().to_owned();
        assert_eq!(
            session.accept(byakko_core::contract::Completion {
                generation: command.generation,
                operation: command.operation,
                payload: CompletionPayload::Macro {
                    slot: slot.clone(),
                    result: FeatureResult::Read(device.read_macro(&slot))
                },
            }),
            Outcome::MacroLoaded
        );
        session
    }
    fn document() -> Document {
        Document {
            format_version: 2,
            backend_id: "memory".into(),
            source_slot: "Spare".into(),
            name: "Imported greeting".into(),
            binding: Some("play-Greeting".into()),
            program: Program {
                repeat_count: 2,
                events: vec![Event {
                    action: Action::Key {
                        usage: 5,
                        pressed: true,
                    },
                    delay_ms: 10,
                }],
            },
        }
    }
    fn import_job(files: &mut Files, session: &Session, path: &Path, document: &Document) -> Job {
        storage::macros::save_new(path, document).unwrap();
        files.form.macro_path = path.display().to_string();
        files.begin(Operation::ImportMacro, session).unwrap()
    }
    fn accept(files: &mut Files, session: &mut Session, job: Job) -> Accepted {
        files
            .accept(job.run(), session)
            .expect("correlated result")
            .expect("successful file job")
    }

    #[test]
    fn real_import_stages_selected_draft_and_returns_source_metadata_without_selecting_it() {
        let mut session = session();
        let mut files = Files::new(&session, None);
        let original = session.macros().unwrap().baseline().cloned();
        let directory = directory();
        let document = document();
        let job = import_job(
            &mut files,
            &session,
            &directory.join("import.json"),
            &document,
        );
        assert!(files.busy());
        assert!(matches!(
            accept(&mut files, &mut session, job),
            Accepted::Imported(_)
        ));
        assert_eq!(session.macros().unwrap().slot(), "Greeting");
        assert_eq!(session.macros().unwrap().baseline(), original.as_ref());
        assert_eq!(session.macros().unwrap().draft(), Some(&document.program));
        assert_eq!(files.form.name("Greeting"), document.name);
        assert_eq!(files.form.name("Spare"), "Spare");
        assert_eq!(
            files.form.bindings.get("Greeting"),
            document.binding.as_ref()
        );
        assert!(files.form.labels_dirty());
        assert!(!files.busy());
        assert!(!session.busy());
    }

    #[test]
    fn cross_backend_portable_import_drops_source_binding_and_invalid_import_is_atomic() {
        let mut session = session();
        let mut files = Files::new(&session, None);
        let directory = directory();
        let mut portable = document();
        portable.backend_id = "portable-source".into();
        let job = import_job(
            &mut files,
            &session,
            &directory.join("portable.json"),
            &portable,
        );
        assert!(matches!(
            accept(&mut files, &mut session, job),
            Accepted::Imported(_)
        ));
        assert_eq!(session.macros().unwrap().draft(), Some(&portable.program));
        assert!(!files.form.bindings.contains_key("Greeting"));
        let draft = session.macros().unwrap().draft().cloned();
        let names = files.form.names.clone();
        let bindings = files.form.bindings.clone();
        let mut invalid = document();
        invalid.program.repeat_count = 0;
        invalid.name = "Must not replace".into();
        let job = import_job(&mut files, &session, &directory.join("zero.json"), &invalid);
        assert!(files.accept(job.run(), &mut session).unwrap().is_err());
        assert_eq!(session.macros().unwrap().draft(), draft.as_ref());
        assert_eq!(files.form.names, names);
        assert_eq!(files.form.bindings, bindings);
        let corrupt = directory.join("corrupt.json");
        fs::write(&corrupt, b"{").unwrap();
        files.form.macro_path = corrupt.display().to_string();
        let job = files.begin(Operation::ImportMacro, &session).unwrap();
        assert!(files.accept(job.run(), &mut session).unwrap().is_err());
        assert_eq!(session.macros().unwrap().draft(), draft.as_ref());
        assert_eq!(files.form.names, names);
        assert!(!files.busy());
    }

    #[test]
    fn export_writes_captured_draft_once_and_success_survives_disconnect() {
        let mut session = session();
        let mut files = Files::new(&session, None);
        let directory = directory();
        let path = directory.join("export.json");
        files.form.macro_path = path.display().to_string();
        files.form.rename("Greeting", "Saved greeting".into());
        let draft = session.macros().unwrap().draft().cloned();
        let job = files.begin(Operation::ExportMacro, &session).unwrap();
        let completion = job.run();
        session.disconnect().unwrap();
        assert!(matches!(
            files.accept(completion, &mut session).unwrap().unwrap(),
            Accepted::Finished(_)
        ));
        let stored = storage::macros::load(&path).unwrap();
        assert_eq!(stored.name, "Saved greeting");
        assert_eq!(stored.source_slot, "Greeting");
        assert_eq!(Some(&stored.program), draft.as_ref());
        let before = fs::read(&path).unwrap();
        files.form.rename("Greeting", "Must not replace".into());
        let job = files.begin(Operation::ExportMacro, &session).unwrap();
        assert!(files.accept(job.run(), &mut session).unwrap().is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!files.busy());
    }

    #[test]
    fn stale_file_ticket_does_not_settle_live_job_and_connection_or_slot_change_rejects_import() {
        let mut session = session();
        let mut files = Files::new(&session, None);
        let directory = directory();
        let job = import_job(
            &mut files,
            &session,
            &directory.join("import.json"),
            &document(),
        );
        let completion = job.run();
        let mut stale = completion.clone();
        stale.ticket.id += 1;
        assert!(files.accept(stale, &mut session).is_none());
        assert!(files.busy());
        let names = files.form.names.clone();
        let draft = session.macros().unwrap().draft().cloned();
        session.disconnect().unwrap();
        session.connect().unwrap();
        assert!(files.accept(completion, &mut session).unwrap().is_err());
        assert_eq!(session.macros().unwrap().draft(), draft.as_ref());
        assert_eq!(files.form.names, names);
        assert!(!files.busy());

        let mut session = self::session();
        let job = import_job(
            &mut files,
            &session,
            &directory.join("changed-slot.json"),
            &document(),
        );
        session.select_macro("Spare").unwrap();
        assert!(files.accept(job.run(), &mut session).unwrap().is_err());
        assert_eq!(session.macros().unwrap().slot(), "Spare");
        assert!(session.macros().unwrap().draft().is_none());
        assert_eq!(files.form.names, names);
    }

    #[test]
    fn labels_load_and_save_real_snapshots_while_retaining_newer_local_names() {
        let mut session = session();
        let directory = directory();
        let mut files = Files::new(&session, Some(directory.clone()));
        assert!(files.needs_labels());
        let job = files.begin(Operation::LoadLabels, &session).unwrap();
        accept(&mut files, &mut session, job);
        assert!(!files.needs_labels());
        assert!(!files.form.labels_dirty());
        files.form.rename("Greeting", "First".into());
        let job = files.begin(Operation::SaveLabels, &session).unwrap();
        files.form.rename("Greeting", "Newer".into());
        accept(&mut files, &mut session, job);
        assert_eq!(files.form.name("Greeting"), "Newer");
        assert!(files.form.labels_dirty());
        assert_eq!(files.form.saved_names["Greeting"], "First");
        let mut reopened = Files::new(&session, Some(directory.clone()));
        let job = reopened.begin(Operation::LoadLabels, &session).unwrap();
        accept(&mut reopened, &mut session, job);
        assert_eq!(reopened.form.name("Greeting"), "First");
        assert!(!reopened.form.labels_dirty());
        let job = files.begin(Operation::SaveLabels, &session).unwrap();
        accept(&mut files, &mut session, job);
        assert!(!files.form.labels_dirty());
        let directory = files.labels_directory.as_ref().unwrap();
        let editor = session.macros().unwrap();
        assert_eq!(
            storage::labels::load_latest(
                directory,
                &editor.capabilities().backend_id,
                &editor.capabilities().slots
            )
            .unwrap()
            .unwrap()["Greeting"],
            "Newer"
        );
    }
}
