//! One device lifecycle and command sequence; feature drafts remain deterministic.
mod archive_ops;
mod host_ops;
mod lighting_ops;
mod macro_files;
mod macro_ops;
mod picture_ops;
mod settings_ops;
pub use macro_files::{FileOperation, FileTicket};
mod recording;

use crate::{Action, Change, Descriptor, State, validate_changes, validate_state};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Bindings = BTreeMap<String, BTreeMap<String, Action>>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Read {
        generation: u64,
        operation: u64,
    },
    Apply {
        generation: u64,
        operation: u64,
        expected: State,
        changes: Vec<Change>,
    },
    ReadMacro {
        generation: u64,
        operation: u64,
        slot: String,
    },
    ApplyMacro {
        generation: u64,
        operation: u64,
        expected: crate::macros::Snapshot,
        desired: crate::macros::Program,
    },
    ReadLighting {
        generation: u64,
        operation: u64,
    },
    ApplyLighting {
        generation: u64,
        operation: u64,
        expected: crate::lighting::Snapshot,
        desired: crate::lighting::Setting,
    },
    ReadPicture {
        generation: u64,
        operation: u64,
    },
    ApplyPicture {
        generation: u64,
        operation: u64,
        expected: crate::picture::Snapshot,
        desired: BTreeMap<String, [u8; 3]>,
    },
    ReadSettings {
        generation: u64,
        operation: u64,
    },
    ApplySetting {
        generation: u64,
        operation: u64,
        expected: crate::settings::Snapshot,
        edit: crate::settings::Edit,
    },
    CaptureArchive {
        generation: u64,
        operation: u64,
    },
    ReviewArchive {
        generation: u64,
        operation: u64,
        target: crate::archive::NativeArchive,
    },
    ApplyArchive {
        generation: u64,
        operation: u64,
        expected: crate::archive::NativeArchive,
        target: crate::archive::NativeArchive,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Completion {
    Read {
        generation: u64,
        operation: u64,
        result: Result<State, String>,
    },
    Apply {
        generation: u64,
        operation: u64,
        result: Result<State, ApplyFailure>,
    },
    ReadMacro {
        generation: u64,
        operation: u64,
        slot: String,
        result: Result<crate::macros::Snapshot, String>,
    },
    ApplyMacro {
        generation: u64,
        operation: u64,
        slot: String,
        result: Result<crate::macros::Snapshot, ApplyFailure>,
    },
    ReadLighting {
        generation: u64,
        operation: u64,
        result: Result<crate::lighting::Snapshot, String>,
    },
    ApplyLighting {
        generation: u64,
        operation: u64,
        result: Result<crate::lighting::Snapshot, ApplyFailure>,
    },
    ReadPicture {
        generation: u64,
        operation: u64,
        result: Result<crate::picture::Snapshot, String>,
    },
    ApplyPicture {
        generation: u64,
        operation: u64,
        result: Result<crate::picture::Snapshot, ApplyFailure>,
    },
    ReadSettings {
        generation: u64,
        operation: u64,
        result: Result<crate::settings::Snapshot, String>,
    },
    ApplySetting {
        generation: u64,
        operation: u64,
        result: Result<crate::settings::Snapshot, ApplyFailure>,
    },
    CaptureArchive {
        generation: u64,
        operation: u64,
        result: Result<crate::archive::NativeArchive, String>,
    },
    ReviewArchive {
        generation: u64,
        operation: u64,
        result: Result<crate::archive::Review, String>,
    },
    ApplyArchive {
        generation: u64,
        operation: u64,
        result: Result<crate::archive::NativeArchive, ApplyFailure>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Recovery {
    Verified,
    Failed,
    NotAttempted,
    /// The executor could not establish whether recovery ran or succeeded.
    Unverified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyFailure {
    pub message: String,
    pub recovery: Recovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostTicket {
    pub generation: u64,
    pub operation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostStart {
    pub ticket: HostTicket,
    pub mode: crate::lighting::HostMode,
    pub setting: Option<crate::lighting::Setting>,
    pub expected: crate::lighting::Snapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostDraft {
    pub mode_id: String,
    pub setting: Option<crate::lighting::Setting>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HostPhase {
    Starting,
    Streaming,
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Problem {
    ReadRequired,
    Read(String),
    Apply(ApplyFailure),
    InvalidApplyResult(String),
    ApplyReadbackMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Disconnected,
    Ready,
    Conflict { device: State },
    Unverified { problem: Problem },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconnectSurface {
    Keymap,
    Macro,
    Lighting,
    Picture,
    Settings,
    Archive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconnectCause<'a> {
    Conflict,
    Apply(&'a ApplyFailure),
    InvalidApplyResult(&'a str),
    ApplyReadbackMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectCaution<'a> {
    pub surface: ReconnectSurface,
    pub cause: ReconnectCause<'a>,
}

fn apply_caution<'a>(
    problem: &'a Problem,
    surface: ReconnectSurface,
) -> Option<ReconnectCaution<'a>> {
    let cause = match problem {
        Problem::ReadRequired | Problem::Read(_) => return None,
        Problem::Apply(failure) => ReconnectCause::Apply(failure),
        Problem::InvalidApplyResult(reason) => ReconnectCause::InvalidApplyResult(reason),
        Problem::ApplyReadbackMismatch => ReconnectCause::ApplyReadbackMismatch,
    };
    Some(ReconnectCaution { surface, cause })
}

fn draft_caution<S>(
    status: &crate::draft::Status<S>,
    surface: ReconnectSurface,
) -> Option<ReconnectCaution<'_>> {
    match status {
        crate::draft::Status::Conflict { .. } => Some(ReconnectCaution {
            surface,
            cause: ReconnectCause::Conflict,
        }),
        crate::draft::Status::Unverified { problem } => apply_caution(problem, surface),
        crate::draft::Status::Unloaded | crate::draft::Status::Ready => None,
    }
}

/// Exactly one device operation can be pending across every editing surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Activity {
    Idle,
    HostLighting {
        ticket: HostTicket,
        phase: HostPhase,
        expected: crate::lighting::Snapshot,
    },
    MacroFile {
        ticket: FileTicket,
    },
    Recording {
        recorder: crate::macros::recorder::Recorder,
    },
    Read {
        operation: u64,
    },
    Apply {
        operation: u64,
    },
    ReadMacro {
        operation: u64,
        slot: String,
    },
    ApplyMacro {
        operation: u64,
        slot: String,
    },
    ReadLighting {
        operation: u64,
    },
    ApplyLighting {
        operation: u64,
    },
    ReadPicture {
        operation: u64,
    },
    ApplyPicture {
        operation: u64,
    },
    ReadSettings {
        operation: u64,
    },
    ApplySetting {
        operation: u64,
    },
    CaptureArchive {
        operation: u64,
    },
    ReviewArchive {
        operation: u64,
        target: crate::archive::NativeArchive,
    },
    ApplyArchive {
        operation: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Acceptance {
    Accepted,
    IgnoredStale,
}

pub struct Session {
    descriptor: Descriptor,
    baseline: Option<State>,
    draft: Option<Bindings>,
    status: Status,
    generation: u64,
    next_operation: u64,
    activity: Activity,
    macros: Option<crate::macros::editor::Editor>,
    lighting: Option<crate::lighting::editor::Editor>,
    host_draft: Option<HostDraft>,
    picture: Option<crate::picture::editor::Editor>,
    settings: Option<crate::settings::editor::Editor>,
    archive_capabilities: Option<crate::archive::ArchiveCapabilities>,
    archive_state: crate::archive::ArchiveState,
}

/// Compatibility name for consumers using only the keymap surface.
pub type KeymapSession = Session;

impl Session {
    pub fn new(descriptor: Descriptor) -> Result<Self, String> {
        let bindings = descriptor
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    descriptor
                        .keys
                        .iter()
                        .map(|key| (key.id.clone(), Action::Disabled))
                        .collect(),
                )
            })
            .collect();
        validate_state(
            &descriptor,
            &State {
                revision: Vec::new(),
                bindings,
            },
        )?;
        Ok(Self {
            descriptor,
            baseline: None,
            draft: None,
            status: Status::Disconnected,
            generation: 0,
            next_operation: 0,
            activity: Activity::Idle,
            macros: None,
            lighting: None,
            host_draft: None,
            picture: None,
            settings: None,
            archive_capabilities: None,
            archive_state: crate::archive::ArchiveState::Idle,
        })
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub fn baseline(&self) -> Option<&State> {
        self.baseline.as_ref()
    }
    pub fn draft(&self) -> Option<&Bindings> {
        self.draft.as_ref()
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    /// Capture this before disconnect, which invalidates feature statuses.
    pub fn reconnect_caution(&self) -> Option<ReconnectCaution<'_>> {
        use crate::archive::{ArchiveProblem, ArchiveState};
        use crate::macros::editor::Status as MacroStatus;

        let keymap = match &self.status {
            Status::Conflict { .. } => Some(ReconnectCaution {
                surface: ReconnectSurface::Keymap,
                cause: ReconnectCause::Conflict,
            }),
            Status::Unverified { problem } => apply_caution(problem, ReconnectSurface::Keymap),
            Status::Disconnected | Status::Ready => None,
        };
        keymap
            .or_else(|| {
                self.macros
                    .as_ref()
                    .and_then(|editor| match editor.status() {
                        MacroStatus::Conflict { .. } => Some(ReconnectCaution {
                            surface: ReconnectSurface::Macro,
                            cause: ReconnectCause::Conflict,
                        }),
                        MacroStatus::Unverified { problem } => {
                            apply_caution(problem, ReconnectSurface::Macro)
                        }
                        MacroStatus::Unloaded | MacroStatus::Ready => None,
                    })
            })
            .or_else(|| {
                self.lighting
                    .as_ref()
                    .and_then(|editor| draft_caution(editor.status(), ReconnectSurface::Lighting))
            })
            .or_else(|| {
                self.picture
                    .as_ref()
                    .and_then(|editor| draft_caution(editor.status(), ReconnectSurface::Picture))
            })
            .or_else(|| {
                self.settings
                    .as_ref()
                    .and_then(|editor| draft_caution(editor.status(), ReconnectSurface::Settings))
            })
            .or_else(|| match &self.archive_state {
                ArchiveState::Unverified { problem, review } => {
                    let cause = match problem {
                        ArchiveProblem::Apply(failure) => ReconnectCause::Apply(failure),
                        ArchiveProblem::ApplyReadbackMismatch => {
                            ReconnectCause::ApplyReadbackMismatch
                        }
                        ArchiveProblem::InvalidResult(reason) if review.is_some() => {
                            ReconnectCause::InvalidApplyResult(reason)
                        }
                        ArchiveProblem::ReadRequired
                        | ArchiveProblem::Capture(_)
                        | ArchiveProblem::Review(_)
                        | ArchiveProblem::InvalidResult(_) => return None,
                    };
                    Some(ReconnectCaution {
                        surface: ReconnectSurface::Archive,
                        cause,
                    })
                }
                ArchiveState::Idle | ArchiveState::Captured(_) | ArchiveState::Ready(_) => None,
            })
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn connect(&mut self) -> Result<u64, String> {
        self.require_idle()?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Connection generation exhausted")?;
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
        self.invalidate_macros();
        self.invalidate_lighting();
        self.invalidate_picture();
        self.invalidate_settings();
        self.invalidate_archive();
        self.select_default_host_mode();
        Ok(self.generation)
    }

    pub fn disconnect(&mut self) {
        if let Activity::Recording { recorder } = &self.activity {
            // No new host timestamp is available on disconnect; release immediately.
            let at = recorder.last_timestamp();
            let _ = self.stop_macro_recording(at);
        }
        self.status = Status::Disconnected;
        self.activity = Activity::Idle;
        self.invalidate_macros();
        self.invalidate_lighting();
        self.invalidate_picture();
        self.invalidate_settings();
        self.invalidate_archive();
    }

    pub fn changes(&self) -> Vec<Change> {
        let (Some(baseline), Some(draft)) = (&self.baseline, &self.draft) else {
            return Vec::new();
        };
        draft
            .iter()
            .flat_map(|(layer, bindings)| {
                bindings
                    .iter()
                    .filter(|(key, action)| {
                        baseline.bindings.get(layer).and_then(|b| b.get(*key)) != Some(*action)
                    })
                    .map(|(key, action)| Change {
                        layer: layer.clone(),
                        key: key.clone(),
                        action: action.clone(),
                    })
            })
            .collect()
    }

    pub fn stage(&mut self, change: Change) -> Result<(), String> {
        if self.busy() || self.status != Status::Ready {
            return Err("Read and verify the connected device before editing".into());
        }
        validate_changes(&self.descriptor, std::slice::from_ref(&change))?;
        let draft = self.draft.as_mut().ok_or("No keymap draft")?;
        *draft
            .get_mut(&change.layer)
            .and_then(|layer| layer.get_mut(&change.key))
            .ok_or("Draft is missing a binding")? = change.action;
        Ok(())
    }

    pub fn revert(&mut self) -> Result<(), String> {
        if self.busy() {
            return Err("Wait for the keymap operation before reverting".into());
        }
        let baseline = self.baseline.as_ref().ok_or("No keymap baseline")?;
        self.draft = Some(baseline.bindings.clone());
        Ok(())
    }

    fn operation(&mut self) -> Result<u64, String> {
        self.next_operation = self
            .next_operation
            .checked_add(1)
            .ok_or("Operation ID exhausted")?;
        Ok(self.next_operation)
    }

    pub fn request_read(&mut self) -> Result<Command, String> {
        if self.busy() || self.status == Status::Disconnected {
            return Err("No connected idle device is available for reading".into());
        }
        let operation = self.operation()?;
        self.activity = Activity::Read { operation };
        self.invalidate_macros();
        Ok(Command::Read {
            generation: self.generation,
            operation,
        })
    }

    pub fn request_apply(&mut self) -> Result<Command, String> {
        if self.busy() || self.status != Status::Ready {
            return Err("Read and verify the device before applying".into());
        }
        let expected = self.baseline.as_ref().ok_or("No keymap baseline")?.clone();
        let changes = self.changes();
        if changes.is_empty() {
            return Err("No keymap changes are staged".into());
        }
        validate_changes(&self.descriptor, &changes)?;
        let operation = self.operation()?;
        self.activity = Activity::Apply { operation };
        self.invalidate_macros();
        self.invalidate_lighting();
        self.invalidate_picture();
        self.invalidate_settings();
        self.invalidate_archive();
        Ok(Command::Apply {
            generation: self.generation,
            operation,
            expected,
            changes,
        })
    }

    pub fn activity(&self) -> &Activity {
        &self.activity
    }
    pub fn busy(&self) -> bool {
        self.activity != Activity::Idle
    }
    pub fn dirty(&self) -> bool {
        !self.changes().is_empty()
            || self.macros.as_ref().is_some_and(|editor| editor.dirty())
            || self.lighting.as_ref().is_some_and(|editor| editor.dirty())
            || self.picture.as_ref().is_some_and(|editor| editor.dirty())
            || self.settings.as_ref().is_some_and(|editor| editor.dirty())
    }

    fn require_idle(&self) -> Result<(), String> {
        if self.busy() {
            Err("Wait for the current device operation".into())
        } else {
            Ok(())
        }
    }

    fn invalidate_macros(&mut self) {
        if let Some(editor) = self.macros.as_mut() {
            editor.invalidate();
        }
    }

    fn invalidate_lighting(&mut self) {
        self.host_draft = None;
        if let Some(editor) = self.lighting.as_mut() {
            editor.invalidate();
        }
    }
    fn invalidate_picture(&mut self) {
        if let Some(editor) = self.picture.as_mut() {
            editor.invalidate();
        }
    }
    fn invalidate_settings(&mut self) {
        if let Some(editor) = self.settings.as_mut() {
            editor.invalidate();
        }
    }

    pub fn accept(&mut self, completion: Completion) -> Acceptance {
        if matches!(&completion, Completion::ReviewArchive { .. }) {
            let Completion::ReviewArchive {
                generation,
                operation,
                result,
            } = completion
            else {
                unreachable!()
            };
            if generation != self.generation
                || !matches!(&self.activity, Activity::ReviewArchive { operation: pending, .. } if *pending == operation)
            {
                return Acceptance::IgnoredStale;
            }
            let Activity::ReviewArchive { target, .. } =
                std::mem::replace(&mut self.activity, Activity::Idle)
            else {
                unreachable!()
            };
            self.accept_archive_review(target, result);
            return Acceptance::Accepted;
        }
        let (generation, expected) = match &completion {
            Completion::Read {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::Read {
                    operation: *operation,
                },
            ),
            Completion::Apply {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::Apply {
                    operation: *operation,
                },
            ),
            Completion::ReadMacro {
                generation,
                operation,
                slot,
                ..
            } => (
                *generation,
                Activity::ReadMacro {
                    operation: *operation,
                    slot: slot.clone(),
                },
            ),
            Completion::ApplyMacro {
                generation,
                operation,
                slot,
                ..
            } => (
                *generation,
                Activity::ApplyMacro {
                    operation: *operation,
                    slot: slot.clone(),
                },
            ),
            Completion::ReadLighting {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::ReadLighting {
                    operation: *operation,
                },
            ),
            Completion::ApplyLighting {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::ApplyLighting {
                    operation: *operation,
                },
            ),
            Completion::ReadPicture {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::ReadPicture {
                    operation: *operation,
                },
            ),
            Completion::ApplyPicture {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::ApplyPicture {
                    operation: *operation,
                },
            ),
            Completion::ReadSettings {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::ReadSettings {
                    operation: *operation,
                },
            ),
            Completion::ApplySetting {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::ApplySetting {
                    operation: *operation,
                },
            ),
            Completion::CaptureArchive {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::CaptureArchive {
                    operation: *operation,
                },
            ),
            Completion::ApplyArchive {
                generation,
                operation,
                ..
            } => (
                *generation,
                Activity::ApplyArchive {
                    operation: *operation,
                },
            ),
            Completion::ReviewArchive { .. } => unreachable!(),
        };
        if generation != self.generation || expected != self.activity {
            return Acceptance::IgnoredStale;
        }
        self.activity = Activity::Idle;
        match completion {
            Completion::Read { result, .. } => self.accept_read(result),
            Completion::Apply { result, .. } => self.accept_apply(result),
            Completion::ReadMacro { result, .. } => self
                .macros
                .as_mut()
                .expect("pending macro capability")
                .accept_read(result),
            Completion::ApplyMacro { result, .. } => self
                .macros
                .as_mut()
                .expect("pending macro capability")
                .accept_apply(result),
            Completion::ReadLighting { result, .. } => self
                .lighting
                .as_mut()
                .expect("pending lighting capability")
                .accept_read(result),
            Completion::ApplyLighting { result, .. } => self
                .lighting
                .as_mut()
                .expect("pending lighting capability")
                .accept_apply(result),
            Completion::ReadPicture { result, .. } => self
                .picture
                .as_mut()
                .expect("pending picture capability")
                .accept_read(result),
            Completion::ApplyPicture { result, .. } => self
                .picture
                .as_mut()
                .expect("pending picture capability")
                .accept_apply(result),
            Completion::ReadSettings { result, .. } => self
                .settings
                .as_mut()
                .expect("pending settings capability")
                .accept_read(result),
            Completion::ApplySetting { result, .. } => self
                .settings
                .as_mut()
                .expect("pending settings capability")
                .accept_apply(result),
            Completion::CaptureArchive { result, .. } => self.accept_archive_capture(result),
            Completion::ApplyArchive { result, .. } => self.accept_archive_apply(result),
            Completion::ReviewArchive { .. } => unreachable!(),
        }
        Acceptance::Accepted
    }

    fn accept_read(&mut self, result: Result<State, String>) {
        let state = match result.and_then(|state| {
            validate_state(&self.descriptor, &state)?;
            Ok(state)
        }) {
            Ok(state) => state,
            Err(reason) => {
                self.status = Status::Unverified {
                    problem: Problem::Read(reason),
                };
                return;
            }
        };
        let dirty = !self.changes().is_empty();
        if dirty && self.baseline.as_ref() != Some(&state) {
            self.status = Status::Conflict { device: state };
            return;
        }
        if !dirty {
            self.draft = Some(state.bindings.clone());
        }
        self.baseline = Some(state);
        self.status = Status::Ready;
    }

    fn accept_apply(&mut self, result: Result<State, ApplyFailure>) {
        let state = match result {
            Ok(state) => state,
            Err(failure) => {
                self.status = Status::Unverified {
                    problem: Problem::Apply(failure),
                };
                return;
            }
        };
        if let Err(reason) = validate_state(&self.descriptor, &state) {
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult(reason),
            };
            return;
        }
        if self.draft.as_ref() != Some(&state.bindings) {
            self.status = Status::Unverified {
                problem: Problem::ApplyReadbackMismatch,
            };
            return;
        }
        self.baseline = Some(state);
        self.status = Status::Ready;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layer, PhysicalKey};

    fn descriptor() -> Descriptor {
        Descriptor {
            backend_id: "memory".into(),
            device_name: "Test keyboard".into(),
            keys: ["a", "b"]
                .into_iter()
                .enumerate()
                .map(|(index, id)| PhysicalKey {
                    id: id.into(),
                    label: id.into(),
                    x: index as f32,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                    visible: true,
                    writable: index == 0,
                })
                .collect(),
            layers: vec![Layer {
                id: "base".into(),
                label: "Base".into(),
            }],
            actions: Vec::new(),
            shortcuts: None,
        }
    }

    #[test]
    fn shortcut_schema_is_checked_when_session_is_created() {
        use crate::{ShortcutCapabilities, UsageChoice};

        let mut descriptor = descriptor();
        descriptor.shortcuts = Some(ShortcutCapabilities {
            modifiers: vec![
                UsageChoice {
                    label: "Ctrl".into(),
                    usage: 224,
                },
                UsageChoice {
                    label: "Shift".into(),
                    usage: 225,
                },
            ],
            keys: vec![UsageChoice {
                label: "C".into(),
                usage: 6,
            }],
            min_modifiers: 1,
            max_modifiers: 2,
        });
        assert!(Session::new(descriptor.clone()).is_ok());
        let shortcuts = descriptor.shortcuts.as_ref().unwrap();
        assert_eq!(
            shortcuts.compose(&[224, 225], 6).unwrap(),
            Action::Shortcut {
                modifiers: vec![224, 225],
                key: 6,
            }
        );
        assert!(shortcuts.compose(&[], 6).is_err());
        assert!(shortcuts.compose(&[224, 224], 6).is_err());
        assert!(shortcuts.compose(&[224, 226], 6).is_err());
        assert!(shortcuts.compose(&[224], 4).is_err());

        let mut invalid = descriptor.clone();
        invalid.shortcuts.as_mut().unwrap().keys.clear();
        assert!(Session::new(invalid).is_err());

        let mut invalid = descriptor.clone();
        invalid.shortcuts.as_mut().unwrap().modifiers[1].usage = 224;
        assert!(Session::new(invalid).is_err());

        let mut invalid = descriptor.clone();
        invalid.shortcuts.as_mut().unwrap().keys[0].label = " ".into();
        assert!(Session::new(invalid).is_err());

        let mut invalid = descriptor.clone();
        invalid.shortcuts.as_mut().unwrap().min_modifiers = 0;
        assert!(Session::new(invalid).is_err());

        let mut invalid = descriptor;
        invalid.shortcuts.as_mut().unwrap().max_modifiers = 3;
        assert!(Session::new(invalid).is_err());
    }

    fn state(revision: u8, usage: u16) -> State {
        State {
            revision: vec![revision],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([
                    ("a".into(), Action::Key(usage)),
                    ("b".into(), Action::Disabled),
                ]),
            )]),
        }
    }

    fn edit(usage: u16) -> Change {
        Change {
            layer: "base".into(),
            key: "a".into(),
            action: Action::Key(usage),
        }
    }

    fn read(session: &mut KeymapSession, result: Result<State, String>) {
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            panic!("expected read command")
        };
        assert_eq!(
            session.accept(Completion::Read {
                generation,
                operation,
                result
            }),
            Acceptance::Accepted
        );
    }

    #[test]
    fn rejects_old_connection_and_out_of_order_completions() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        let first_generation = session.connect().unwrap();
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(generation, first_generation);
        session.disconnect();
        session.connect().unwrap();
        let Command::Read {
            generation: next_generation,
            operation: next_operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::Read {
                generation,
                operation,
                result: Ok(state(1, 4))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept(Completion::Read {
                generation: next_generation,
                operation: next_operation + 1,
                result: Ok(state(2, 4))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept(Completion::Apply {
                generation: next_generation,
                operation: next_operation,
                result: Ok(state(2, 4))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.activity(),
            &Activity::Read {
                operation: next_operation
            }
        );
        assert!(session.baseline().is_none());
        assert_eq!(
            session.accept(Completion::Read {
                generation: next_generation,
                operation: next_operation,
                result: Ok(state(2, 4))
            }),
            Acceptance::Accepted
        );
        assert_eq!(session.status(), &Status::Ready);
    }

    #[test]
    fn dirty_reconnect_matching_read_retains_draft_and_difference_conflicts() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        assert!(
            session
                .stage(Change {
                    key: "b".into(),
                    ..edit(5)
                })
                .is_err()
        );
        session.stage(edit(5)).unwrap();
        session.disconnect();
        assert!(session.stage(edit(6)).is_err());
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(session.changes(), vec![edit(5)]);
        session.disconnect();
        session.connect().unwrap();
        read(&mut session, Ok(state(2, 6)));
        assert_eq!(
            session.status(),
            &Status::Conflict {
                device: state(2, 6)
            }
        );
        assert_eq!(session.baseline(), Some(&state(1, 4)));
        assert_eq!(session.draft().unwrap()["base"]["a"], Action::Key(5));
        assert!(session.request_apply().is_err());
    }

    #[test]
    fn failed_and_mismatched_apply_keep_baseline_and_draft_unverified() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        session.stage(edit(5)).unwrap();
        let Command::Apply {
            generation,
            operation,
            expected,
            changes,
        } = session.request_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(expected, state(1, 4));
        assert_eq!(changes, vec![edit(5)]);
        let failure = ApplyFailure {
            message: "write failed".into(),
            recovery: Recovery::Verified,
        };
        assert_eq!(
            session.accept(Completion::Apply {
                generation,
                operation,
                result: Err(failure.clone())
            }),
            Acceptance::Accepted
        );
        assert_eq!(
            session.status(),
            &Status::Unverified {
                problem: Problem::Apply(failure)
            }
        );
        assert_eq!(session.baseline(), Some(&state(1, 4)));
        assert_eq!(session.changes(), vec![edit(5)]);
        assert!(session.request_apply().is_err());
        let mut malformed = state(1, 4);
        malformed.bindings.get_mut("base").unwrap().remove("b");
        read(&mut session, Ok(malformed));
        assert!(matches!(
            session.status(),
            Status::Unverified {
                problem: Problem::Read(_)
            }
        ));
        assert_eq!(session.changes(), vec![edit(5)]);
        read(&mut session, Ok(state(1, 4)));
        let Command::Apply {
            generation,
            operation,
            ..
        } = session.request_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::Apply {
                generation,
                operation,
                result: Ok(state(2, 6))
            }),
            Acceptance::Accepted
        );
        assert_eq!(
            session.status(),
            &Status::Unverified {
                problem: Problem::ApplyReadbackMismatch
            }
        );
        assert_eq!(session.changes(), vec![edit(5)]);
    }

    #[test]
    fn failed_read_preserves_draft_and_matching_apply_becomes_ready() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        session.stage(edit(5)).unwrap();
        read(&mut session, Err("transport unavailable".into()));
        assert_eq!(
            session.status(),
            &Status::Unverified {
                problem: Problem::Read("transport unavailable".into())
            }
        );
        assert_eq!(session.baseline(), Some(&state(1, 4)));
        assert_eq!(session.changes(), vec![edit(5)]);
        assert!(session.request_apply().is_err());
        read(&mut session, Ok(state(1, 4)));
        let Command::Apply {
            generation,
            operation,
            ..
        } = session.request_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::Apply {
                generation,
                operation,
                result: Ok(state(2, 5))
            }),
            Acceptance::Accepted
        );
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(session.baseline(), Some(&state(2, 5)));
        assert!(session.changes().is_empty());
    }

    #[test]
    fn commands_and_results_round_trip_as_owned_json_values() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        let command = session.request_read().unwrap();
        let encoded = serde_json::to_vec(&command).unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&encoded).unwrap(),
            command
        );
        let Command::Read {
            generation,
            operation,
        } = command
        else {
            unreachable!()
        };
        let completion = Completion::Read {
            generation,
            operation,
            result: Ok(state(1, 4)),
        };
        let encoded = serde_json::to_vec(&completion).unwrap();
        let decoded = serde_json::from_slice::<Completion>(&encoded).unwrap();
        assert_eq!(decoded, completion);
        assert_eq!(session.accept(decoded), Acceptance::Accepted);
        session.stage(edit(5)).unwrap();
        let apply = session.request_apply().unwrap();
        let encoded = serde_json::to_vec(&apply).unwrap();
        assert_eq!(serde_json::from_slice::<Command>(&encoded).unwrap(), apply);
        let Command::Apply {
            generation,
            operation,
            ..
        } = apply
        else {
            unreachable!()
        };
        let failure = Completion::Apply {
            generation,
            operation,
            result: Err(ApplyFailure {
                message: "write failed".into(),
                recovery: Recovery::Failed,
            }),
        };
        let encoded = serde_json::to_vec(&failure).unwrap();
        assert_eq!(
            serde_json::from_slice::<Completion>(&encoded).unwrap(),
            failure
        );
    }

    #[test]
    fn opaque_action_survives_apply_command_and_verified_completion() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        let opaque = Action::Opaque {
            backend_id: "memory".into(),
            data: vec![0, 255, 7, 42],
            label: "Unknown action".into(),
        };
        session
            .stage(Change {
                action: opaque.clone(),
                ..edit(4)
            })
            .unwrap();
        let command = session.request_apply().unwrap();
        let encoded = serde_json::to_vec(&command).unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&encoded).unwrap(),
            command
        );
        let Command::Apply {
            generation,
            operation,
            expected,
            changes,
        } = command
        else {
            unreachable!()
        };
        assert_eq!(expected.revision, [1]);
        assert_eq!(changes[0].action, opaque);
        let mut applied = state(2, 4);
        applied
            .bindings
            .get_mut("base")
            .unwrap()
            .insert("a".into(), opaque);
        let completion = Completion::Apply {
            generation,
            operation,
            result: Ok(applied.clone()),
        };
        let encoded = serde_json::to_vec(&completion).unwrap();
        assert_eq!(
            serde_json::from_slice::<Completion>(&encoded).unwrap(),
            completion
        );
        assert_eq!(session.accept(completion), Acceptance::Accepted);
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(session.baseline(), Some(&applied));
    }
}
