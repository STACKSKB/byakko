//! File paths and local names are frontend input, never device configuration.
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    ImportMacro,
    ExportMacro,
    LoadLabels,
    SaveLabels,
    ExportArchive,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nia87_playback_modes_share_the_named_slot_and_advertised_numbering() {
        let session = byakko_devices::nia87::application::session().unwrap();
        let caps = session.macros().unwrap().capabilities();
        let mut names = Form::default();
        let slot = &caps.slots[0];
        assert_eq!(slot.label, "Macro 1");
        for binding in caps
            .bindings
            .iter()
            .filter(|binding| binding.slot == slot.id)
        {
            assert_eq!(
                names.assignment_name(&binding.action, caps),
                Some("Macro 1")
            );
        }
        names.names.insert(slot.id.clone(), "Launch editor".into());
        for binding in caps
            .bindings
            .iter()
            .filter(|binding| binding.slot == slot.id)
        {
            assert_eq!(
                names.assignment_name(&binding.action, caps),
                Some("Launch editor")
            );
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    MacroPath(String),
    ArchivePath(String),
    Name(String),
    Begin(Operation),
    Capture,
}

#[derive(Default)]
pub struct Form {
    pub macro_path: String,
    pub archive_path: String,
    pub(crate) names: BTreeMap<String, String>,
    pub(crate) saved_names: BTreeMap<String, String>,
    pub(crate) bindings: BTreeMap<String, String>,
}

impl Form {
    /// Binding actions identify slots through advertised metadata, not wire indices.
    pub fn assignment_name<'a>(
        &'a self,
        action: &byakko_core::model::keymap::Action,
        capabilities: &'a byakko_core::model::macros::Capabilities,
    ) -> Option<&'a str> {
        let binding = capabilities
            .bindings
            .iter()
            .find(|binding| binding.action == *action)?;
        self.names
            .get(&binding.slot)
            .filter(|name| !name.trim().is_empty())
            .map(String::as_str)
            .or_else(|| {
                capabilities
                    .slots
                    .iter()
                    .find(|slot| slot.id == binding.slot)
                    .map(|slot| slot.label.as_str())
            })
    }
    pub fn name(&self, slot: &str) -> &str {
        self.names.get(slot).map_or("", String::as_str)
    }
    pub fn labels_dirty(&self) -> bool {
        self.names != self.saved_names
    }
    pub fn rename(&mut self, slot: &str, name: String) {
        if let Some(current) = self.names.get_mut(slot) {
            *current = name;
        }
    }
}
