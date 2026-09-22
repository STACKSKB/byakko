//! Macro-facing session operations. The parent owns the one operation sequence.
use super::{Activity, Command, Problem, Session, Status};
use crate::{
    Change,
    macros::{Capabilities, Edit, editor::Editor},
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

    pub fn request_macro_apply(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let (expected, desired) = self.macro_editor()?.request_apply()?;
        let operation = self.operation()?;
        self.activity = Activity::ApplyMacro {
            operation,
            slot: expected.slot.clone(),
        };
        // Do not let an earlier keymap baseline authorize a later binding write
        // after another part of the device has been changed or failed mid-write.
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
        Ok(Command::ApplyMacro {
            generation: self.generation,
            operation,
            expected,
            desired,
        })
    }
}
