//! Local paths and asynchronous file effects. Validation/staging remain in core.
use super::{Closing, Desktop, Message as AppMessage};
use byakko_core::{
    macros::{
        Document,
        editor::{Editor, Status},
    },
    session::{Acceptance, FileOperation, FileTicket},
};
use byakko_devices::macro_files;
use iced::{
    Element, Task,
    widget::{button, column, row, text, text_input},
};
use std::{collections::BTreeMap, path::PathBuf};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(super) enum Message {
    Toggle,
    Path(String),
    Name(String),
    Begin(FileOperation),
    Complete(FileTicket, Result<Option<Box<Document>>, String>),
}

#[derive(Default)]
pub(super) struct Fields {
    expanded: bool,
    path: String,
    metadata: BTreeMap<String, Metadata>,
}

struct Metadata {
    name: String,
    binding: Option<String>,
}

impl Fields {
    pub(super) fn remember_binding(&mut self, editor: &Editor, binding: &str) {
        let name = self.name(editor).to_owned();
        self.metadata
            .entry(editor.slot().into())
            .or_insert(Metadata {
                name,
                binding: None,
            })
            .binding = Some(binding.into());
    }
    fn name<'a>(&'a self, editor: &'a Editor) -> &'a str {
        self.metadata.get(editor.slot()).map_or_else(
            || {
                editor
                    .capabilities()
                    .slots
                    .iter()
                    .find(|slot| slot.id == editor.slot())
                    .map_or(editor.slot(), |slot| slot.label.as_str())
            },
            |metadata| metadata.name.as_str(),
        )
    }

    fn document(&self, editor: &Editor) -> Result<Document, String> {
        Ok(Document {
            format_version: 2,
            backend_id: editor.capabilities().backend_id.clone(),
            source_slot: editor.slot().into(),
            name: self.name(editor).into(),
            binding: self
                .metadata
                .get(editor.slot())
                .and_then(|metadata| metadata.binding.clone()),
            program: editor.draft().ok_or("No editable macro to export")?.clone(),
        })
    }
}

impl Desktop {
    pub(super) fn update_macro_files(&mut self, message: Message) -> Task<AppMessage> {
        if let Message::Complete(ticket, result) = message {
            return self.complete_macro_file(ticket, result);
        }
        if self.busy() {
            return Task::none();
        }
        match message {
            Message::Toggle => self.macro_files.expanded = !self.macro_files.expanded,
            Message::Path(value) => self.macro_files.path = value,
            Message::Name(value) => {
                if let Some(editor) = self.session.macros() {
                    self.macro_files
                        .metadata
                        .entry(editor.slot().into())
                        .or_insert(Metadata {
                            name: String::new(),
                            binding: None,
                        })
                        .name = value;
                }
            }
            Message::Begin(kind) => return self.begin_macro_file(kind),
            Message::Complete(..) => unreachable!("completion handled above"),
        }
        Task::none()
    }

    fn begin_macro_file(&mut self, kind: FileOperation) -> Task<AppMessage> {
        if self.macro_files.path.trim().is_empty() {
            self.notice = Some("Enter a macro file path".into());
            return Task::none();
        }
        let path = PathBuf::from(self.macro_files.path.trim());
        let ticket = match self.session.begin_macro_file(kind) {
            Ok(ticket) => ticket,
            Err(reason) => {
                self.notice = Some(reason);
                return Task::none();
            }
        };
        self.notice = None;
        match kind {
            FileOperation::Import => file_task(ticket, move || {
                macro_files::load(&path).map(|document| Some(Box::new(document)))
            }),
            FileOperation::Export => {
                let document = self
                    .session
                    .macros()
                    .ok_or_else(|| "Macros are unavailable".to_string())
                    .and_then(|editor| self.macro_files.document(editor));
                file_task(ticket, move || {
                    document
                        .and_then(|doc| macro_files::save_new(&path, &doc))
                        .map(|()| None)
                })
            }
        }
    }

    fn complete_macro_file(
        &mut self,
        ticket: FileTicket,
        result: Result<Option<Box<Document>>, String>,
    ) -> Task<AppMessage> {
        let (program, metadata, result) = match result {
            Ok(Some(document)) => {
                let binding = document
                    .binding
                    .as_ref()
                    .filter(|id| {
                        document.backend_id == self.session.descriptor().backend_id
                            && self.session.macros().is_some_and(|editor| {
                                editor.capabilities().bindings.iter().any(|binding| {
                                    binding.slot == ticket.slot && &binding.id == *id
                                })
                            })
                    })
                    .cloned();
                let notice = if document.binding.is_some() && binding.is_none() {
                    "Imported draft. The source binding does not apply to this slot; choose a binding separately."
                } else {
                    "Imported into the selected slot's draft. Review before saving."
                };
                (
                    Some(document.program),
                    Some(Metadata {
                        name: document.name,
                        binding,
                    }),
                    Ok(notice),
                )
            }
            Ok(None) => (None, None, Ok("Exported the draft to a new macro file.")),
            Err(error) => (None, None, Err(error)),
        };
        match self.session.finish_macro_file(&ticket, program) {
            Ok(Acceptance::IgnoredStale) => return Task::none(),
            Err(error) => {
                self.notice = Some(error);
                self.closing = Closing::Open;
                return Task::none();
            }
            Ok(Acceptance::Accepted) => {}
        }
        match result {
            Err(error) => {
                self.notice = Some(error);
                self.closing = Closing::Open;
            }
            Ok(message) => {
                if let Some(metadata) = metadata {
                    self.macro_files.metadata.insert(ticket.slot, metadata);
                    self.reset_macro_inputs();
                }
                self.notice = Some(message.into());
                if self.closing == Closing::Waiting {
                    return self.close();
                }
            }
        }
        Task::none()
    }
}

// Blocking filesystem calls must not occupy Iced's async subscription executor.
fn file_task(
    ticket: FileTicket,
    work: impl FnOnce() -> Result<Option<Box<Document>>, String> + Send + 'static,
) -> Task<AppMessage> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    if let Err(error) = std::thread::Builder::new()
        .name("byakko-file".into())
        .spawn(move || {
            let _ = sender.send(work());
        })
    {
        return Task::done(AppMessage::File(Message::Complete(
            ticket,
            Err(error.to_string()),
        )));
    }
    Task::perform(
        async move {
            receiver.await.unwrap_or_else(|_| Err("File worker stopped without a result; check the destination before retrying an export".into()))
        },
        move |result| AppMessage::File(Message::Complete(ticket, result)),
    )
}

pub(super) fn view<'a>(app: &'a Desktop, editor: &'a Editor) -> Element<'a, AppMessage> {
    let toggle = button("Macro file…")
        .on_press_maybe((!app.busy()).then_some(AppMessage::File(Message::Toggle)));
    if !app.macro_files.expanded {
        return toggle.into();
    }
    let editable = !app.busy() && editor.draft().is_some();
    column![
        row![
            toggle,
            text("Local file · import replaces the draft · export creates a new file")
        ]
        .spacing(8),
        text_input("Macro name (file metadata)", app.macro_files.name(editor))
            .on_input_maybe(editable.then_some(|value| AppMessage::File(Message::Name(value)))),
        row![
            text_input("Path to macro JSON", &app.macro_files.path).on_input_maybe(
                (!app.busy()).then_some(|value| AppMessage::File(Message::Path(value)))
            ),
            button("Import draft").on_press_maybe(
                (editable && *editor.status() == Status::Ready)
                    .then_some(AppMessage::File(Message::Begin(FileOperation::Import)))
            ),
            button("Export new file").on_press_maybe(
                editable.then_some(AppMessage::File(Message::Begin(FileOperation::Export)))
            ),
        ]
        .spacing(8),
    ]
    .spacing(6)
    .into()
}
