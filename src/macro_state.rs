//! One macro slot's draft, paired decoded/raw baseline, and worker lifecycle.

use crate::macros::{self, Macro};

struct BaselineImage {
    decoded: Macro,
    raw: Vec<u8>,
}

enum Baseline {
    Unloaded,
    Verified(BaselineImage),
    Unverified(BaselineImage),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Idle,
    Reading(u8),
    Applying(u8),
}

pub(crate) struct ApplyRequest {
    pub slot: u8,
    pub expected: Vec<u8>,
    pub draft: Macro,
}

pub(crate) struct MacroState {
    slot: u8,
    draft: Macro,
    baseline: Baseline,
    operation: Operation,
}

impl MacroState {
    pub(crate) fn new() -> Self {
        Self {
            slot: 0,
            draft: Macro {
                repeat_count: 1,
                events: Vec::new(),
            },
            baseline: Baseline::Unloaded,
            operation: Operation::Idle,
        }
    }

    pub(crate) fn slot(&self) -> u8 {
        self.slot
    }
    pub(crate) fn draft(&self) -> &Macro {
        &self.draft
    }
    /// UI and recorder may edit this draft only while no worker is active.
    pub(crate) fn draft_mut(&mut self) -> &mut Macro {
        &mut self.draft
    }
    #[cfg(test)]
    pub(crate) fn operation(&self) -> Operation {
        self.operation
    }
    pub(crate) fn busy(&self) -> bool {
        self.operation != Operation::Idle
    }
    pub(crate) fn loaded(&self) -> bool {
        self.baseline().is_some()
    }
    pub(crate) fn trusted(&self) -> bool {
        matches!(self.baseline, Baseline::Verified(_))
    }
    pub(crate) fn dirty(&self) -> bool {
        self.baseline()
            .is_some_and(|baseline| baseline.decoded != self.draft)
    }

    fn baseline(&self) -> Option<&BaselineImage> {
        match &self.baseline {
            Baseline::Verified(value) | Baseline::Unverified(value) => Some(value),
            Baseline::Unloaded => None,
        }
    }

    pub(crate) fn mark_unverified(&mut self) {
        self.baseline = match std::mem::replace(&mut self.baseline, Baseline::Unloaded) {
            Baseline::Verified(value) | Baseline::Unverified(value) => Baseline::Unverified(value),
            Baseline::Unloaded => Baseline::Unloaded,
        };
    }

    pub(crate) fn switch_slot(&mut self, next: u8) -> Result<bool, String> {
        if next >= 50 {
            return Err("Macro slot must be 0..=49.".into());
        }
        if next == self.slot {
            return Ok(false);
        }
        if self.busy() {
            return Err("Finish the macro operation before changing slots.".into());
        }
        if self.dirty() {
            return Err("Save or revert this draft before changing slots.".into());
        }
        self.slot = next;
        self.baseline = Baseline::Unloaded;
        self.draft = Macro {
            repeat_count: 1,
            events: Vec::new(),
        };
        Ok(true)
    }

    pub(crate) fn begin_read(&mut self) -> Result<u8, String> {
        if self.busy() || self.dirty() {
            return Err("Finish work or revert the draft before loading.".into());
        }
        self.operation = Operation::Reading(self.slot);
        Ok(self.slot)
    }

    pub(crate) fn begin_apply(&mut self) -> Result<ApplyRequest, String> {
        if self.busy() || !self.trusted() || !self.dirty() {
            return Err("No applicable macro draft is ready.".into());
        }
        let expected = self
            .baseline()
            .ok_or("Load this macro slot before applying changes.")?
            .raw
            .clone();
        macros::encode(&self.draft)?;
        self.operation = Operation::Applying(self.slot);
        Ok(ApplyRequest {
            slot: self.slot,
            expected,
            draft: self.draft.clone(),
        })
    }

    pub(crate) fn complete_read(
        &mut self,
        slot: u8,
        result: Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        if self.operation != Operation::Reading(slot) || slot != self.slot {
            self.mark_unverified();
            return Err(
                "Unexpected macro operation or slot in worker result; draft preserved.".into(),
            );
        }
        self.operation = Operation::Idle;
        let bytes = result.map_err(|error| {
            self.mark_unverified();
            format!("Slot {slot} read failed: {error}")
        })?;
        let decoded = macros::decode(&bytes).map_err(|error| {
            self.mark_unverified();
            format!("Slot {slot} could not be decoded: {error}")
        })?;
        self.draft = decoded.clone();
        self.baseline = Baseline::Verified(BaselineImage {
            decoded,
            raw: bytes,
        });
        Ok(())
    }

    pub(crate) fn complete_apply(
        &mut self,
        slot: u8,
        result: Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        if self.operation != Operation::Applying(slot) || slot != self.slot {
            self.mark_unverified();
            return Err(
                "Unexpected macro operation or slot in worker result; draft preserved.".into(),
            );
        }
        self.operation = Operation::Idle;
        let bytes = result.map_err(|error| {
            self.mark_unverified();
            format!("Slot {slot} apply failed: {error}")
        })?;
        if macros::encode(&self.draft).as_ref() != Ok(&bytes) {
            self.mark_unverified();
            return Err("Macro worker returned unexpected bytes; device state is unverified. Draft retained.".into());
        }
        self.baseline = Baseline::Verified(BaselineImage {
            decoded: self.draft.clone(),
            raw: bytes,
        });
        Ok(())
    }

    pub(crate) fn revert(&mut self) -> bool {
        if self.busy() {
            return false;
        }
        if let Some(decoded) = self.baseline().map(|baseline| baseline.decoded.clone()) {
            self.draft = decoded;
            true
        } else {
            false
        }
    }

    pub(crate) fn can_import(&self) -> Result<(), String> {
        if self.busy() {
            return Err("Finish the macro operation before importing.".into());
        }
        if !self.loaded() {
            return Err(
                "Load this slot before importing, so its current bytes are backed up before apply."
                    .into(),
            );
        }
        if self.dirty() {
            return Err("Save or revert the current draft before importing another file.".into());
        }
        Ok(())
    }

    pub(crate) fn import_draft(&mut self, draft: Macro) -> Result<(), String> {
        self.can_import()?;
        macros::encode(&draft)?;
        self.draft = draft;
        Ok(())
    }

    pub(crate) fn clear_draft(&mut self) -> Result<(), String> {
        if self.busy() || !self.loaded() {
            return Err(
                "Load a macro slot and finish its operation before clearing the draft.".into(),
            );
        }
        self.draft.events.clear();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn seed_verified(&mut self, macro_data: Macro) {
        let raw = macros::encode(&macro_data).unwrap();
        self.draft = macro_data.clone();
        self.baseline = Baseline::Verified(BaselineImage {
            decoded: macro_data,
            raw,
        });
    }
}

impl Default for MacroState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::MacroEvent;

    fn changed(state: &mut MacroState) {
        state.draft_mut().events.push(MacroEvent::Key {
            usage: 4,
            down: true,
            delay_ms: 0,
        });
    }

    #[test]
    fn clear_is_reversible_and_cannot_replace_a_pending_operation() {
        let mut state = MacroState::new();
        assert!(state.clear_draft().is_err());
        changed(&mut state);
        state.draft_mut().repeat_count = 3;
        let original = state.draft().clone();
        state.seed_verified(original.clone());
        state.clear_draft().unwrap();
        assert!(state.draft().events.is_empty());
        assert_eq!(state.draft().repeat_count, 3);
        assert!(state.dirty() && state.trusted());
        assert!(state.revert());
        assert_eq!(state.draft(), &original);
        state.begin_read().unwrap();
        assert!(state.clear_draft().is_err());
        assert_eq!(state.draft(), &original);
    }

    #[test]
    fn wrong_slot_and_error_keep_paired_baseline_but_remove_trust() {
        let mut state = MacroState::new();
        state.seed_verified(state.draft().clone());
        changed(&mut state);
        state.begin_apply().unwrap();
        assert!(state.complete_apply(1, Ok(vec![0; 256])).is_err());
        assert!(state.loaded() && state.dirty() && !state.trusted());
        assert_eq!(state.operation(), Operation::Applying(0));
        assert!(!state.revert());
        assert!(
            state
                .complete_apply(0, Err("readback failed".into()))
                .is_err()
        );
        assert_eq!(state.operation(), Operation::Idle);
        assert!(state.loaded() && state.dirty() && !state.trusted());
        assert!(state.revert());
        assert!(!state.dirty());
    }

    #[test]
    fn wrong_kind_does_not_finish_pending_read() {
        let mut state = MacroState::new();
        assert_eq!(state.begin_read().unwrap(), 0);
        assert!(state.complete_apply(0, Ok(vec![0; 256])).is_err());
        assert_eq!(state.operation(), Operation::Reading(0));
        let raw = macros::encode(state.draft()).unwrap();
        state.complete_read(0, Ok(raw)).unwrap();
        assert_eq!(state.operation(), Operation::Idle);
        assert!(state.trusted());
    }

    #[test]
    fn failed_and_undecodable_reads_keep_the_paired_baseline_and_draft() {
        let mut state = MacroState::new();
        state.seed_verified(state.draft().clone());
        let original = state.draft().clone();
        state.begin_read().unwrap();
        assert!(state.complete_read(0, Err("device gone".into())).is_err());
        assert!(state.loaded() && !state.trusted());
        assert_eq!(state.draft(), &original);
        assert_eq!(state.operation(), Operation::Idle);

        state.begin_read().unwrap();
        assert!(state.complete_read(0, Ok(vec![0; 1])).is_err());
        assert!(state.loaded() && !state.trusted());
        assert_eq!(state.draft(), &original);
        assert_eq!(state.operation(), Operation::Idle);

        let raw = macros::encode(&original).unwrap();
        state.begin_read().unwrap();
        state.complete_read(0, Ok(raw)).unwrap();
        assert!(state.trusted() && !state.dirty());
    }

    #[test]
    fn invalid_slot_and_busy_import_revert_are_rejected() {
        let mut state = MacroState::new();
        assert!(state.switch_slot(50).is_err());
        state.seed_verified(state.draft().clone());
        state.begin_read().unwrap();
        assert!(state.can_import().is_err());
        assert!(!state.revert());
        assert!(state.switch_slot(1).is_err());
    }

    #[test]
    fn matching_apply_verifies_and_mismatched_bytes_preserve_draft() {
        let mut state = MacroState::new();
        state.seed_verified(state.draft().clone());
        changed(&mut state);
        let request = state.begin_apply().unwrap();
        assert_eq!(request.slot, 0);
        assert!(state.complete_apply(0, Ok(vec![0; 256])).is_err());
        assert!(!state.trusted() && state.dirty());
        assert!(state.begin_apply().is_err());
        assert!(state.revert());
        let raw = macros::encode(state.draft()).unwrap();
        state.begin_read().unwrap();
        state.complete_read(0, Ok(raw)).unwrap();
        changed(&mut state);
        state.begin_apply().unwrap();
        state
            .complete_apply(0, Ok(macros::encode(state.draft()).unwrap()))
            .unwrap();
        assert!(state.trusted() && !state.dirty());
    }
}
