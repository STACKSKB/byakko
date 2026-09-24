//! Translate desktop inputs to shared session operations. No device effects here.
use super::{
    Desktop,
    macro_form::{Form, Input, number},
    macro_view::Composer,
};
use byakko_core::macros::{Content, Edit, editor::Status as MacroStatus};
use byakko_core::session::Command;

#[derive(Clone, Debug)]
pub(super) enum Message {
    ChooseBinding(String),
    Assign(String),
    Add,
    Select(String),
    ReadCatalog,
    Read,
    Apply,
    Revert,
    Edit(Edit),
    Inspect(usize),
    Form(Input),
    StageEvent,
    NewEvent,
    RepeatInput(String),
    StageRepeat,
    ToggleComposer,
}

impl Desktop {
    pub(super) fn reset_macro_inputs(&mut self) {
        self.macro_form = Form::default();
        self.macro_composer = Composer::Collapsed;
        self.repeat_input = self
            .session
            .macros()
            .and_then(|editor| editor.draft())
            .map_or_else(String::new, |program| program.repeat_count.to_string());
    }

    /// Prepare an explicitly added, empty slot whose stored count cannot be
    /// staged through the editor. Other stored snapshots remain untouched.
    pub(super) fn initialize_new_macro(&mut self) {
        let count = {
            let Some(slot) = self.macro_new_slot.as_deref() else {
                return;
            };
            let Some(editor) = self.session.macros() else {
                return;
            };
            if editor.slot() != slot || *editor.status() != MacroStatus::Ready || editor.dirty() {
                return;
            }
            let Some(snapshot) = editor.baseline().filter(|snapshot| snapshot.slot == slot) else {
                return;
            };
            let Content::Editable(program) = &snapshot.content else {
                return;
            };
            if !program.events.is_empty()
                || editor
                    .capabilities()
                    .editable_repeat_counts
                    .contains(&program.repeat_count)
            {
                return;
            }
            *editor.capabilities().editable_repeat_counts.start()
        };
        self.macro_notice = self.session.edit_macro(Edit::Repeat(count)).err();
        if self.macro_notice.is_none() {
            self.reset_macro_inputs();
        }
    }

    pub(super) fn update_macro(&mut self, message: Message) {
        if self.busy() {
            return;
        }
        self.macro_notice = None;
        match message {
            Message::Add => {
                let catalog_complete = self
                    .session
                    .macros()
                    .is_some_and(|editor| editor.catalog().is_some());
                let slot = if catalog_complete || self.macro_new_slot.is_none() {
                    self.session.next_free_macro_slot().or_else(|| {
                        (!catalog_complete)
                            .then(|| self.session.next_macro_candidate_after(None))
                            .flatten()
                    })
                } else {
                    self.session
                        .next_macro_candidate_after(self.macro_new_slot.as_deref())
                }
                .map(str::to_owned);
                let Some(slot) = slot else {
                    self.macro_notice = Some("No further unbound macro slot is available".into());
                    return;
                };
                if let Err(reason) = self.session.select_macro(&slot) {
                    self.macro_notice = Some(reason);
                    return;
                }
                self.macro_new_slot = Some(slot);
                self.reset_macro_inputs();
                if !self.session.macros().is_some_and(|editor| {
                    editor.status() == &byakko_core::macros::editor::Status::Ready
                        && editor
                            .baseline()
                            .is_some_and(|snapshot| snapshot.slot == editor.slot())
                }) {
                    let request = self.session.request_macro_read();
                    self.submit_macro(request);
                } else {
                    self.initialize_new_macro();
                }
            }
            Message::ReadCatalog => {
                let request = self.session.request_macro_catalog_read();
                self.submit_macro(request);
            }
            Message::ChooseBinding(binding) => {
                let Some(editor) = self.session.macros() else {
                    return;
                };
                if editor
                    .capabilities()
                    .bindings
                    .iter()
                    .any(|choice| choice.slot == editor.slot() && choice.id == binding)
                {
                    self.macro_binding_choice = Some((editor.slot().to_owned(), binding));
                    self.macro_notice = None;
                }
            }
            Message::Assign(binding) => {
                self.macro_notice = None;
                let Some(editor) = self.session.macros() else {
                    self.macro_notice = Some("Read a macro before assigning it".into());
                    return;
                };
                if self.macro_binding_choice.as_ref()
                    != Some(&(editor.slot().to_owned(), binding.clone()))
                {
                    self.macro_notice = Some("Choose a playback mode before assigning".into());
                    return;
                }
                if !self.session.changes().is_empty() {
                    self.macro_notice = Some(
                        "Save or revert other key assignments before assigning this macro".into(),
                    );
                    return;
                }
                let Some(key) = self.selected.as_deref() else {
                    self.macro_notice = Some("Select a writable key on the keyboard first".into());
                    return;
                };
                if let Err(reason) = self.session.stage_macro_binding(&self.layer, key, &binding) {
                    self.macro_notice = Some(reason);
                    return;
                }
                match self.session.request_apply() {
                    Ok(command) => {
                        if let Some(editor) = self.session.macros() {
                            self.macro_files.remember_binding(editor, &binding);
                        }
                        self.submit_macro(Ok(command));
                    }
                    Err(reason) => {
                        let _ = self.session.revert();
                        self.macro_notice = Some(reason);
                    }
                }
            }
            Message::Select(slot) => {
                if self.session.macros().is_some_and(|editor| {
                    editor.slot() == slot && *editor.status() == MacroStatus::Ready
                }) {
                    return;
                }
                let changed = self
                    .session
                    .macros()
                    .is_some_and(|editor| editor.slot() != slot);
                if let Err(reason) = self.session.select_macro(&slot) {
                    self.macro_notice = Some(reason);
                    return;
                }
                if changed {
                    self.macro_new_slot = None;
                    self.reset_macro_inputs();
                }
                let request = self.session.request_macro_read();
                self.submit_macro(request);
            }
            Message::Read => {
                let request = self.session.request_macro_read();
                self.submit_macro(request);
            }
            Message::Apply => {
                let request = self.session.request_macro_apply();
                self.submit_macro(request);
            }
            Message::Revert => {
                self.macro_notice = self.session.revert_macro().err();
                if self.macro_notice.is_none() {
                    self.reset_macro_inputs();
                }
            }
            Message::Edit(edit) => self.stage_macro(edit),
            Message::Inspect(index) => {
                if let Some(editor) = self.session.macros()
                    && let Some(event) = editor.draft().and_then(|draft| draft.events.get(index))
                {
                    self.macro_form = Form::from_event(index, event, editor.capabilities());
                    self.macro_composer = Composer::Expanded;
                }
            }
            Message::Form(input) => self.macro_form.update(input),
            Message::NewEvent => {
                self.macro_form = Form::default();
                self.macro_composer = Composer::Expanded;
            }
            Message::ToggleComposer => {
                self.macro_composer = match self.macro_composer {
                    Composer::Collapsed => Composer::Expanded,
                    Composer::Expanded => Composer::Collapsed,
                };
            }
            Message::StageEvent => match self.event_edit() {
                Ok(edit) => self.stage_macro(edit),
                Err(reason) => self.macro_notice = Some(reason),
            },
            Message::RepeatInput(value) => self.repeat_input = value,
            Message::StageRepeat => match number(&self.repeat_input, "Repeat count") {
                Ok(count) => {
                    self.macro_notice = self.session.edit_macro(Edit::Repeat(count)).err();
                    if self.macro_notice.is_none() {
                        // Changing the count leaves the event being composed intact.
                        self.repeat_input = count.to_string();
                    }
                }
                Err(reason) => self.macro_notice = Some(reason),
            },
        }
    }

    fn stage_macro(&mut self, edit: Edit) {
        self.macro_notice = self.session.edit_macro(edit).err();
        if self.macro_notice.is_none() {
            self.reset_macro_inputs();
        }
    }

    fn submit_macro(&mut self, request: Result<Command, String>) {
        self.submit(request);
        self.macro_notice = self.notice.take();
    }

    fn event_edit(&self) -> Result<Edit, String> {
        let editor = self.session.macros().ok_or("Macros are unavailable")?;
        let event = self.macro_form.event(editor.capabilities())?;
        Ok(match self.macro_form.target {
            Some(at) => Edit::Replace { at, event },
            None => Edit::Insert {
                at: editor
                    .draft()
                    .ok_or("Read an editable macro first")?
                    .events
                    .len(),
                event,
            },
        })
    }
}
