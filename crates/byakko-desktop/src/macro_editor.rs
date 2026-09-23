//! Translate desktop inputs to shared session operations. No device effects here.
use super::{
    Desktop,
    macro_form::{Form, Input, number},
};
use byakko_core::macros::Edit;

#[derive(Clone, Debug)]
pub(super) enum Message {
    Bind(String),
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
}

impl Desktop {
    pub(super) fn reset_macro_inputs(&mut self) {
        self.macro_form = Form::default();
        self.repeat_input = self
            .session
            .macros()
            .and_then(|editor| editor.draft())
            .map_or_else(String::new, |program| program.repeat_count.to_string());
    }

    pub(super) fn update_macro(&mut self, message: Message) {
        if self.busy() {
            return;
        }
        self.notice = None;
        match message {
            Message::Add => {
                let slot = self.session.next_free_macro_slot().map(str::to_owned);
                let Some(slot) = slot else {
                    self.notice = Some("No free macro slot is available".into());
                    return;
                };
                if let Err(reason) = self.session.select_macro(&slot) {
                    self.notice = Some(reason);
                    return;
                }
                self.macro_new_slot = Some(slot);
                self.reset_macro_inputs();
                let request = self.session.request_macro_read();
                self.submit(request);
            }
            Message::ReadCatalog => {
                let request = self.session.request_macro_catalog_read();
                self.submit(request);
            }
            Message::Bind(binding) => {
                self.notice = match self.selected.as_deref() {
                    Some(key) => self
                        .session
                        .stage_macro_binding(&self.layer, key, &binding)
                        .err(),
                    None => Some("Select a writable key on the Keys page first".into()),
                };
                if self.notice.is_none() {
                    if let Some(editor) = self.session.macros() {
                        self.macro_files.remember_binding(editor, &binding);
                    }
                    self.page = super::Page::Keys;
                }
            }
            Message::Select(slot) => {
                let changed = self
                    .session
                    .macros()
                    .is_some_and(|editor| editor.slot() != slot);
                if let Err(reason) = self.session.select_macro(&slot) {
                    self.notice = Some(reason);
                    return;
                }
                self.macro_new_slot = None;
                if changed {
                    self.reset_macro_inputs();
                }
                let request = self.session.request_macro_read();
                self.submit(request);
            }
            Message::Read => {
                let request = self.session.request_macro_read();
                self.submit(request);
            }
            Message::Apply => {
                let request = self.session.request_macro_apply();
                self.submit(request);
            }
            Message::Revert => {
                self.notice = self.session.revert_macro().err();
                if self.notice.is_none() {
                    self.reset_macro_inputs();
                }
            }
            Message::Edit(edit) => self.stage_macro(edit),
            Message::Inspect(index) => {
                if let Some(editor) = self.session.macros()
                    && let Some(event) = editor.draft().and_then(|draft| draft.events.get(index))
                {
                    self.macro_form = Form::from_event(index, event, editor.capabilities());
                }
            }
            Message::Form(input) => self.macro_form.update(input),
            Message::NewEvent => self.macro_form = Form::default(),
            Message::StageEvent => match self.event_edit() {
                Ok(edit) => self.stage_macro(edit),
                Err(reason) => self.notice = Some(reason),
            },
            Message::RepeatInput(value) => self.repeat_input = value,
            Message::StageRepeat => match number(&self.repeat_input, "Repeat count") {
                Ok(count) => {
                    self.notice = self.session.edit_macro(Edit::Repeat(count)).err();
                    if self.notice.is_none() {
                        // Changing the count leaves the event being composed intact.
                        self.repeat_input = count.to_string();
                    }
                }
                Err(reason) => self.notice = Some(reason),
            },
        }
    }

    fn stage_macro(&mut self, edit: Edit) {
        self.notice = self.session.edit_macro(edit).err();
        if self.notice.is_none() {
            self.reset_macro_inputs();
        }
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
