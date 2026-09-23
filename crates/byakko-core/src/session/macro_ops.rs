//! Macro-facing session operations. The parent owns the one operation sequence.
use super::{Activity, Command, Problem, Session, Status};
use crate::{
    Change,
    macros::{Capabilities, Choice, Edit, editor::Editor},
};
#[cfg(test)]
mod tests;

impl Session {
    /// Configure capabilities at composition time, before the first connection.
    pub fn with_macros(mut self, capabilities: Capabilities) -> Result<Self, String> {
        if self.generation != 0 || self.macros.is_some() {
            return Err("Macro capabilities must be supplied once before connecting".into());
        }
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Macro capabilities belong to a different backend".into());
        }
        self.macros = Some(Editor::new(capabilities)?);
        Ok(self)
    }

    pub fn macros(&self) -> Option<&Editor> {
        self.macros.as_ref()
    }

    pub fn macro_catalog_scanning(&self) -> bool {
        self.macro_catalog_operation.is_some()
    }

    /// Stored programs and slots still referenced by either keymap image.
    /// A blank but bound slot must not be silently reused by Add.
    pub fn macro_library_slots(&self) -> Option<Vec<&Choice>> {
        let editor = self.macros.as_ref()?;
        let configured: std::collections::BTreeSet<_> = editor
            .configured_slots()?
            .into_iter()
            .map(|slot| slot.id.as_str())
            .collect();
        Some(
            editor
                .capabilities()
                .slots
                .iter()
                .filter(|slot| {
                    configured.contains(slot.id.as_str()) || self.macro_slot_bound(&slot.id)
                })
                .collect(),
        )
    }

    pub fn next_free_macro_slot(&self) -> Option<&str> {
        if self.status != Status::Ready {
            return None;
        }
        let editor = self.macros.as_ref()?;
        let configured: std::collections::BTreeSet<_> = editor
            .configured_slots()?
            .into_iter()
            .map(|slot| slot.id.as_str())
            .collect();
        editor
            .capabilities()
            .slots
            .iter()
            .find(|slot| !configured.contains(slot.id.as_str()) && !self.macro_slot_bound(&slot.id))
            .map(|slot| slot.id.as_str())
    }

    fn macro_slot_bound(&self, slot: &str) -> bool {
        let Some(editor) = self.macros.as_ref() else {
            return false;
        };
        let actions: Vec<_> = editor
            .capabilities()
            .bindings
            .iter()
            .filter(|binding| binding.slot == slot)
            .map(|binding| &binding.action)
            .collect();
        let contains = |bindings: &super::Bindings| {
            bindings
                .values()
                .flat_map(|layer| layer.values())
                .any(|action| actions.contains(&action))
        };
        self.baseline
            .as_ref()
            .is_some_and(|state| contains(&state.bindings))
            || self.draft.as_ref().is_some_and(contains)
    }

    fn macro_editor(&mut self) -> Result<&mut Editor, String> {
        self.require_idle()?;
        self.macros
            .as_mut()
            .ok_or_else(|| "Device does not support macro editing".into())
    }

    pub fn select_macro(&mut self, slot: &str) -> Result<(), String> {
        self.macro_editor()?.select(slot)
    }

    pub fn edit_macro(&mut self, edit: Edit) -> Result<(), String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.macro_editor()?.edit(edit)
    }

    pub fn stage_macro_binding(
        &mut self,
        layer: &str,
        key: &str,
        binding: &str,
    ) -> Result<(), String> {
        self.require_idle()?;
        let action = self
            .macros
            .as_ref()
            .ok_or("Device does not support macro editing")?
            .binding_action(binding)?;
        self.stage(Change {
            layer: layer.into(),
            key: key.into(),
            action,
        })
    }

    pub fn revert_macro(&mut self) -> Result<(), String> {
        self.macro_editor()?.revert()
    }

    pub fn request_macro_read(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let slot = self.macro_editor()?.slot().to_owned();
        let operation = self.operation()?;
        self.activity = Activity::ReadMacro {
            operation,
            slot: slot.clone(),
        };
        Ok(Command::ReadMacro {
            generation: self.generation,
            operation,
            slot,
        })
    }

    pub fn request_macro_catalog_read(&mut self) -> Result<Command, String> {
        self.require_idle()?;
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        if self.macro_catalog_operation.is_some() {
            return Err("Macro catalog scan is already in progress".into());
        }
        let slots = self
            .macros
            .as_ref()
            .ok_or("Device does not support macro editing")?
            .capabilities()
            .slots
            .iter()
            .map(|choice| choice.id.clone())
            .collect();
        let operation = self.operation()?;
        self.macro_catalog_operation = Some(operation);
        Ok(Command::ReadMacroCatalog {
            generation: self.generation,
            operation,
            slots,
        })
    }

    pub fn request_macro_apply(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let (expected, desired) = self.macro_editor()?.request_apply()?;
        let operation = self.operation()?;
        self.macro_catalog_operation = None;
        self.activity = Activity::ApplyMacro {
            operation,
            slot: expected.slot.clone(),
        };
        // Do not let an earlier keymap baseline authorize a later binding write
        // after another part of the device has been changed or failed mid-write.
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
        self.invalidate_lighting();
        self.invalidate_picture();
        self.invalidate_settings();
        self.invalidate_archive();
        Ok(Command::ApplyMacro {
            generation: self.generation,
            operation,
            expected,
            desired,
        })
    }
}
