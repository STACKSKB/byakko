//! Macro-facing session operations. The parent owns the one operation sequence.
use super::{Activity, Command, Session, Status};
use crate::session::CommandPayload;
use crate::session::{DeviceActivity, Feature};
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

    /// Known bindings remain navigable while the storage catalog is scanning.
    pub fn macro_bound_slots(&self) -> Option<Vec<&Choice>> {
        let editor = self.macros.as_ref()?;
        Some(
            editor
                .capabilities()
                .slots
                .iter()
                .filter(|slot| self.macro_slot_bound(&slot.id))
                .collect(),
        )
    }

    /// A foreground candidate is only a slot to read, never proof it is free.
    pub fn next_macro_candidate_after(&self, after: Option<&str>) -> Option<&str> {
        if self.status != Status::Ready {
            return None;
        }
        let slots = &self.macros.as_ref()?.capabilities().slots;
        let start = after
            .and_then(|id| slots.iter().position(|slot| slot.id == id))
            .map_or(0, |index| index + 1);
        slots
            .iter()
            .skip(start)
            .find(|slot| !self.macro_slot_bound(&slot.id))
            .map(|slot| slot.id.as_str())
    }

    pub fn next_free_macro_slot(&self) -> Option<&str> {
        if self.status != Status::Ready {
            return None;
        }
        let editor = self.macros.as_ref()?;
        if let Some(configured) = editor.configured_slots() {
            let configured: std::collections::BTreeSet<_> = configured
                .into_iter()
                .map(|slot| slot.id.as_str())
                .collect();
            return editor
                .capabilities()
                .slots
                .iter()
                .find(|slot| {
                    !configured.contains(slot.id.as_str()) && !self.macro_slot_bound(&slot.id)
                })
                .map(|slot| slot.id.as_str());
        }
        // A foreground read can prove one slot free before the background
        // catalog has reached every slot. Never infer another slot is empty.
        matches!(
            editor.baseline(),
            Some(snapshot) if snapshot.slot == editor.slot()
                && matches!(&snapshot.content, crate::macros::Content::Editable(program) if program.events.is_empty())
        )
        .then(|| editor.slot())
        .filter(|slot| editor.status() == &crate::macros::editor::Status::Ready && !self.macro_slot_bound(slot))
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
        self.activity = Activity::Device {
            operation,
            request: DeviceActivity::Read(Feature::Macro { slot: slot.clone() }),
        };
        Ok(Command {
            generation: self.generation,
            operation,
            payload: CommandPayload::ReadMacro { slot },
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
        Ok(Command {
            generation: self.generation,
            operation,
            payload: CommandPayload::ReadMacroCatalog { slots },
        })
    }

    pub fn request_macro_apply(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let (expected, desired) = self.macro_editor()?.request_apply()?;
        let operation = self.operation()?;
        self.macro_catalog_operation = None;
        self.activity = Activity::Device {
            operation,
            request: DeviceActivity::Apply(Feature::Macro {
                slot: expected.slot.clone(),
            }),
        };
        self.invalidate_archive();
        Ok(Command {
            generation: self.generation,
            operation,
            payload: CommandPayload::ApplyMacro { expected, desired },
        })
    }
}
