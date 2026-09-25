//! Correlate a macro save with the key binding it authorizes.
use super::Desktop;
use byakko_core::session::CompletionPayload;
use byakko_core::session::FeatureResult;
use byakko_core::{
    Change,
    macros::editor::Status as MacroStatus,
    session::{Completion, Status},
};

#[derive(Clone, Debug)]
pub(super) struct Target {
    slot: String,
    binding: String,
    layer: String,
    key: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Ticket {
    generation: u64,
    operation: u64,
}

#[derive(Clone, Debug)]
pub(super) enum Pending {
    Saving { ticket: Ticket, target: Target },
    Assigning { ticket: Ticket, target: Target },
}

pub(super) enum Progress {
    Saved,
    SaveFailed(String),
    Assigned,
    AssignFailed(String),
}

impl Desktop {
    /// A projection for both the button state and the action's final guard.
    pub(super) fn assignment_problem(&self, binding: &str) -> Option<String> {
        self.validate_macro_assignment(binding).err()
    }

    fn validate_macro_assignment(&self, binding: &str) -> Result<Target, String> {
        if self.busy() || self.macro_assignment.is_some() {
            return Err("Wait for the current device operation".into());
        }
        if self.session.status() != &Status::Ready {
            return Err("Read and verify the keymap before assigning".into());
        }
        if !self.session.changes().is_empty() {
            return Err("Save or revert other key assignments before assigning this macro".into());
        }
        let key = self
            .selected
            .as_ref()
            .ok_or("Select a writable key on the keyboard first")?;
        let editor = self
            .session
            .macros()
            .ok_or("Device does not support macro editing")?;
        if editor.status() != &MacroStatus::Ready {
            return Err("Read and verify the macro before assigning".into());
        }
        let slot = editor.slot();
        if editor
            .baseline()
            .is_none_or(|snapshot| snapshot.slot != slot)
        {
            return Err("Read this macro slot before assigning".into());
        }
        let program = editor.draft().ok_or("Macro is not editable")?;
        if editor.dirty() {
            editor.request_apply()?;
        }
        if !editor
            .capabilities()
            .editable_repeat_counts
            .contains(&program.repeat_count)
        {
            return Err("Stored macro count is outside the editable range".into());
        }
        let input_count = super::macro_form::number::<u32>(&self.repeat_input, "Repeat count")?;
        if input_count != program.repeat_count {
            return Err("Finish editing the repeat count before saving and assigning".into());
        }
        let choice = editor
            .capabilities()
            .bindings
            .iter()
            .find(|choice| choice.slot == slot && choice.id == binding)
            .ok_or("Choose a playback mode before assigning")?;
        if choice
            .required_repeat_count
            .is_some_and(|count| count != program.repeat_count)
        {
            return Err("Macro repeat count does not match this playback mode".into());
        }
        byakko_core::validate_changes(
            self.session.descriptor(),
            &[Change {
                layer: self.layer.clone(),
                key: key.clone(),
                action: choice.action.clone(),
            }],
        )?;
        if self
            .session
            .draft()
            .and_then(|layers| layers.get(&self.layer))
            .and_then(|keys| keys.get(key))
            .is_none()
        {
            return Err("Read the keymap before assigning".into());
        }
        Ok(Target {
            slot: slot.into(),
            binding: binding.into(),
            layer: self.layer.clone(),
            key: key.clone(),
        })
    }

    pub(super) fn save_and_assign_macro(&mut self, binding: String) {
        self.macro_notice = None;
        let target = match self.validate_macro_assignment(&binding) {
            Ok(target) => target,
            Err(reason) => {
                self.macro_notice = Some(reason);
                return;
            }
        };
        if self.session.macros().is_some_and(|editor| editor.dirty()) {
            match self.session.request_macro_apply() {
                Ok(command) => {
                    self.macro_assignment = Some(Pending::Saving {
                        ticket: Ticket {
                            generation: command.generation,
                            operation: command.operation,
                        },
                        target,
                    });
                    self.submit(Ok(command));
                    if !self.session.busy() {
                        self.macro_assignment = None;
                        self.macro_notice = Some(self.notice.take().unwrap_or_else(|| {
                            "Macro save could not start; key was not assigned".into()
                        }));
                    }
                }
                Err(reason) => self.macro_notice = Some(reason),
            }
        } else {
            self.assign_saved_macro(target);
        }
    }

    fn assign_saved_macro(&mut self, target: Target) {
        if self.session.status() != &Status::Ready
            || !self.session.macros().is_some_and(|editor| {
                editor.slot() == target.slot
                    && editor.status() == &MacroStatus::Ready
                    && !editor.dirty()
            })
        {
            self.macro_notice = Some("Macro save was not verified; key was not assigned".into());
            return;
        }
        if let Err(reason) =
            self.session
                .stage_macro_binding(&target.layer, &target.key, &target.binding)
        {
            self.macro_notice = Some(format!("Macro saved; key was not assigned: {reason}"));
            return;
        }
        // The selected key may already contain this binding. The macro is still saved.
        if self.session.changes().is_empty() {
            self.remember_macro_assignment(&target.binding);
            return;
        }
        match self.session.request_apply() {
            Ok(command) => {
                self.macro_assignment = Some(Pending::Assigning {
                    ticket: Ticket {
                        generation: command.generation,
                        operation: command.operation,
                    },
                    target,
                });
                self.submit(Ok(command));
                if !self.session.busy() {
                    self.macro_assignment = None;
                    self.macro_notice = Some(format!(
                        "Macro saved; key assignment could not start: {}",
                        self.notice
                            .take()
                            .unwrap_or_else(|| "device operation rejected".into())
                    ));
                }
            }
            Err(reason) => {
                let _ = self.session.revert();
                self.macro_notice = Some(format!("Macro saved; key was not assigned: {reason}"));
            }
        }
    }

    fn remember_macro_assignment(&mut self, binding: &str) {
        if let Some(editor) = self.session.macros() {
            self.macro_files.remember_binding(editor, binding);
        }
    }

    pub(super) fn cancel_macro_assignment_after_disconnect(&mut self) {
        self.macro_notice = match self.macro_assignment.take() {
            Some(Pending::Saving { .. }) =>
                Some("Macro save was interrupted; key was not assigned. Read the macro before retrying.".into()),
            Some(Pending::Assigning { .. }) =>
                Some("Macro saved; key assignment was interrupted and is not verified. Read the keymap before retrying.".into()),
            None => return,
        };
        self.closing = super::Closing::Open;
    }

    pub(super) fn macro_assignment_completion(&self, completion: &Completion) -> Option<Progress> {
        match (self.macro_assignment.as_ref()?, completion) {
            (
                Pending::Saving { ticket, target },
                Completion {
                    generation,
                    operation,
                    payload:
                        CompletionPayload::Macro {
                            slot,
                            result: FeatureResult::Apply(result),
                        },
                },
            ) if ticket.generation == *generation
                && ticket.operation == *operation
                && target.slot == *slot =>
            {
                Some(match result {
                    Ok(_) => Progress::Saved,
                    Err(failure) => Progress::SaveFailed(failure.message.clone()),
                })
            }
            (
                Pending::Assigning { ticket, .. },
                Completion {
                    generation,
                    operation,
                    payload: CompletionPayload::Keymap(FeatureResult::Apply(result)),
                },
            ) if ticket.generation == *generation && ticket.operation == *operation => {
                Some(match result {
                    Ok(_) => Progress::Assigned,
                    Err(failure) => Progress::AssignFailed(failure.message.clone()),
                })
            }
            _ => None,
        }
    }

    pub(super) fn advance_macro_assignment(&mut self, progress: Progress) {
        let Some(pending) = self.macro_assignment.take() else {
            return;
        };
        match (pending, progress) {
            (Pending::Saving { target, .. }, Progress::Saved) => self.assign_saved_macro(target),
            (Pending::Saving { .. }, Progress::SaveFailed(reason)) => {
                self.macro_notice =
                    Some(format!("Macro save failed; key was not assigned: {reason}"));
            }
            (Pending::Assigning { target, .. }, Progress::Assigned) => {
                if self.session.status() == &Status::Ready {
                    self.remember_macro_assignment(&target.binding);
                    self.macro_notice = None;
                } else {
                    self.macro_notice = Some("Macro saved; key assignment was not verified".into());
                }
            }
            (Pending::Assigning { .. }, Progress::AssignFailed(reason)) => {
                self.macro_notice = Some(format!("Macro saved; key assignment failed: {reason}"));
            }
            _ => unreachable!("progress and pending operation have different phases"),
        }
        // A macro can be saved while the following key operation cannot begin.
        // Do not let an earlier Close request treat that partial success as done.
        if self.macro_notice.is_some() {
            self.closing = super::Closing::Open;
        }
    }
}
