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
