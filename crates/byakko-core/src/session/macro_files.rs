//! Correlation and exclusion for local file work. Core never sees a path or file.
use super::{Acceptance, Activity, Session};
use crate::macros::{Program, editor::Status};
use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FileOperation {
    Import,
    Export,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileTicket {
    pub generation: u64,
    pub operation: u64,
    pub slot: String,
    pub kind: FileOperation,
}

impl Session {
    pub fn begin_macro_file(&mut self, kind: FileOperation) -> Result<FileTicket, String> {
        self.require_idle()?;
        let editor = self.macros.as_ref().ok_or("Macros are unavailable")?;
        if editor.draft().is_none() {
            return Err("No editable macro to import or export".into());
        }
        if kind == FileOperation::Import && *editor.status() != Status::Ready {
            return Err("Read and verify the target macro before importing".into());
        }
        let slot = editor.slot().to_owned();
        let ticket = FileTicket {
            generation: self.generation,
            operation: self.operation()?,
            slot,
            kind,
        };
        self.activity = Activity::MacroFile {
            ticket: ticket.clone(),
        };
        Ok(ticket)
    }

    /// None releases an export or failed/cancelled import. Some stages an import atomically.
    pub fn finish_macro_file(
        &mut self,
        ticket: &FileTicket,
        imported: Option<Program>,
    ) -> Result<Acceptance, String> {
        if self.generation != ticket.generation
            || self.activity
                != (Activity::MacroFile {
                    ticket: ticket.clone(),
                })
        {
            return Ok(Acceptance::IgnoredStale);
        }
        self.activity = Activity::Idle;
        if let Some(program) = imported {
            if ticket.kind != FileOperation::Import {
                return Err("Export cannot replace a macro draft".into());
            }
            self.macros
                .as_mut()
                .ok_or("Macros are unavailable")?
                .replace(program)?;
        }
        Ok(Acceptance::Accepted)
    }
}
