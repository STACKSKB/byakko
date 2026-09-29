//! Pure browser transaction planner. JavaScript performs only the effect named
//! by each step and returns its result; all report bytes and outcomes stay here.

use std::collections::VecDeque;

use byakko_core::{
    contract::{
        ApplyFailure, Command, CommandPayload, Completion, CompletionPayload, FeatureCommand,
        FeatureResult, Recovery,
    },
    model::{keymap, lighting, macros, picture, settings},
};
use byakko_protocol::nia87::{
    adapter, archive_adapter, configuration, lighting as native_lighting, lighting_adapter,
    macro_adapter, macros as native_macros, picture_adapter, protocol, recovery_keymaps,
    settings as native_settings, settings_adapter,
};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

#[derive(Clone)]
enum ReplyTag {
    Version,
    Profile,
    Base,
    Function,
    Lighting,
    PictureContext,
    PicturePage,
    MacroPage,
    Setting(u8),
}

#[derive(Clone)]
enum Step {
    Backup(Value),
    Exchange {
        report: Vec<u8>,
        tag: ReplyTag,
    },
    Write {
        report: Vec<u8>,
        before_ms: u32,
        after_ms: u32,
    },
    Complete(Completion),
}

#[derive(Default)]
struct Replies {
    version: Option<[u8; 64]>,
    profile: Option<[u8; 64]>,
    base: Vec<[u8; 64]>,
    function: Vec<[u8; 64]>,
    lighting: Option<[u8; 64]>,
    context: Option<[u8; 64]>,
    picture: Vec<[u8; 64]>,
    macro_pages: Vec<[u8; 64]>,
    settings: Vec<[u8; 64]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyStage {
    Main,
    ObserveFn,
    RestoreFn,
    ObserveBase,
    RestoreBase,
    Verify,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum MacroStage {
    Main,
    Restore,
    Verify,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsStage {
    Main,
    Restore,
    Verify,
}
#[derive(Clone, Copy)]
enum ArchiveStage {
    Keymap,
    Lighting,
    Settings,
    Picture,
    Macro(u8),
}

#[derive(Default)]
struct ArchiveCapture {
    keymaps: Option<adapter::Snapshot>,
    lighting: Option<native_lighting::Lighting>,
    settings: Option<native_settings::Settings>,
    picture: Option<Vec<[u8; 3]>>,
    macros: Vec<Vec<u8>>,
}

enum Plan {
    ReadKeymap,
    ReadLighting,
    ReadSettings,
    ReadArchive {
        stage: ArchiveStage,
        capture: ArchiveCapture,
    },
    ReadPicture,
    ReadMacro {
        slot: String,
    },
    ReadCatalog {
        slots: Vec<String>,
        next: usize,
        results: Vec<macros::Snapshot>,
    },
    ApplyKeymap {
        before: adapter::Snapshot,
        target: adapter::Snapshot,
        stage: KeyStage,
        cause: Option<String>,
    },
    ApplyMacro {
        slot: String,
        number: u8,
        before: Vec<u8>,
        target: Vec<u8>,
        stage: MacroStage,
        cause: Option<String>,
    },
    ApplyLighting {
        submitted: lighting::Snapshot,
    },
    ApplySettings {
        before: native_settings::Settings,
        target: native_settings::Settings,
        restore_report: [u8; 64],
        edit: settings::Edit,
        stage: SettingsStage,
        cause: Option<String>,
    },
    ApplyPicture {
        submitted: picture::Snapshot,
    },
    Terminal,
}

#[wasm_bindgen]
pub struct BrowserOperation {
    command: Command,
    plan: Plan,
    queue: VecDeque<Step>,
    current: Step,
    replies: Replies,
    setter_attempted: bool,
    yield_between_catalog_slots: bool,
}

#[wasm_bindgen]
impl BrowserOperation {
    fn backup(&mut self, feature: &str, before: Value, target: Value) {
        self.queue.push_back(Step::Backup(json!({
            "format_version": 1,
            "feature": feature,
            "generation": self.command.generation,
            "operation": self.command.operation,
            "before": before,
            "target": target,
        })));
    }

    fn start_keymap(&mut self, expected: keymap::State, desired: Vec<keymap::Change>) {
        let result = adapter::to_snapshot(&expected).and_then(|before| {
            let target = adapter::draft_snapshot(&expected, &desired)?;
            if before.firmware != 0x0100 || before.profile != 0 {
                return Err(
                    "Firmware/profile differs from validated Nia87 0x0100/profile 0".into(),
                );
            }
            Ok((before, target))
        });
        let (before, target) = match result {
            Ok(value) => value,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        if before == target {
            self.complete(CompletionPayload::Keymap(FeatureResult::Apply(Ok(
                expected,
            ))));
            return;
        }
        self.backup("keymap", json!(before), json!(target));
        if let Err(error) = self.queue_key_changes(&before, &target, false) {
            self.failed(error, Recovery::NotAttempted);
            return;
        }
        if let Err(error) = self.queue_key_changes(&before, &target, true) {
            self.failed(error, Recovery::NotAttempted);
            return;
        }
        self.plan = Plan::ApplyKeymap {
            before,
            target,
            stage: KeyStage::Main,
            cause: None,
        };
        self.keymap_read();
    }

    fn queue_key_changes(
        &mut self,
        before: &adapter::Snapshot,
        target: &adapter::Snapshot,
        function: bool,
    ) -> Result<(), String> {
        let (old, new, index) = if function {
            (&before.function, &target.function, 0)
        } else {
            (&before.base, &target.base, before.profile)
        };
        for slot in 0..126 {
            if old[slot] != new[slot] {
                self.write(
                    protocol::single_key_report(function, index, slot, new[slot])?,
                    0,
                    1000,
                );
            }
        }
        Ok(())
    }

    fn start_macro(&mut self, expected: macros::Snapshot, desired: macros::Program) {
        let result = macro_adapter::prepare(&expected, &desired).and_then(|(before, value)| {
            let number = macro_adapter::slot_number(&expected.slot)?;
            let target = native_macros::encode(&value)?;
            Ok((number, before.as_bytes().to_vec(), target))
        });
        let (number, before, target) = match result {
            Ok(value) => value,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        if before == target {
            self.complete(CompletionPayload::Macro {
                slot: expected.slot.clone(),
                result: FeatureResult::Apply(Ok(expected)),
            });
            return;
        }
        let reports = match native_macros::write_reports(number, &target) {
            Ok(reports) => reports,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        self.backup(
            &format!("macro-{}", expected.slot),
            json!({"slot":expected.slot,"bytes":before}),
            json!(target),
        );
        for (page, report) in reports.into_iter().enumerate() {
            self.write(report, 0, if page == 4 { 2030 } else { 30 });
        }
        self.plan = Plan::ApplyMacro {
            slot: expected.slot,
            number,
            before,
            target,
            stage: MacroStage::Main,
            cause: None,
        };
        self.macro_read(number);
    }

    fn start_lighting(&mut self, expected: lighting::Snapshot, desired: lighting::Setting) {
        let result = lighting_adapter::draft(&expected, &desired).and_then(|setting| {
            let original = native_lighting::Lighting::decode(&expected.revision)?;
            if original.recognized_setting().is_none() {
                return Err("Lighting baseline is not a recognized Nia87 LED response".into());
            }
            let report = native_lighting::write_report(&setting)?;
            Ok(report)
        });
        let report = match result {
            Ok(report) => report,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        if expected.revision[1..8] == report[1..8] {
            self.complete(CompletionPayload::Lighting(FeatureResult::Apply(Ok(
                expected,
            ))));
            return;
        }
        let mut submitted_bytes = expected.revision.clone();
        submitted_bytes[1..8].copy_from_slice(&report[1..8]);
        let submitted = lighting_adapter::from_bytes(&submitted_bytes).map(|mut snapshot| {
            snapshot.evidence = lighting::Evidence::TransportAccepted;
            snapshot
        });
        let submitted = match submitted {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        self.backup(
            "lighting",
            json!({"before":expected,"target_report":report.as_slice()}),
            json!(submitted),
        );
        self.write(report, 0, 500);
        self.plan = Plan::ApplyLighting { submitted };
    }

    fn start_settings(&mut self, expected: settings::Snapshot, desired: settings::Edit) {
        let (before, planned) = match settings_adapter::prepare(&expected, &desired) {
            Ok(value) => value,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        if before == planned.target {
            self.complete(CompletionPayload::Settings(FeatureResult::Apply(Ok(
                expected,
            ))));
            return;
        }
        self.backup("settings", json!(before), json!(planned.target));
        self.write(planned.report, 0, 500);
        self.plan = Plan::ApplySettings {
            before,
            target: planned.target,
            restore_report: planned.restore_report,
            edit: desired,
            stage: SettingsStage::Main,
            cause: None,
        };
        self.settings_read();
    }

    fn start_picture(
        &mut self,
        expected: picture::Snapshot,
        desired: std::collections::BTreeMap<String, [u8; 3]>,
    ) {
        let (original, target, context) = match picture_adapter::desired_native(&expected, &desired)
        {
            Ok(value) => value,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        if original == target {
            self.complete(CompletionPayload::Picture(FeatureResult::Apply(Ok(
                expected,
            ))));
            return;
        }
        let reports = match native_lighting::user_picture_write_reports(&target) {
            Ok(reports) => reports,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        let submitted = picture_adapter::project(&target, context).map(|mut snapshot| {
            snapshot.evidence = picture::Evidence::TransportAccepted;
            snapshot
        });
        let submitted = match submitted {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.failed(error, Recovery::NotAttempted);
                return;
            }
        };
        self.backup(
            "picture",
            json!({"colors":original,"context":context}),
            json!(target),
        );
        for report in reports {
            self.write(report, 20, 0);
        }
        self.plan = Plan::ApplyPicture { submitted };
    }

    fn begin_key_recovery(
        &mut self,
        before: adapter::Snapshot,
        target: adapter::Snapshot,
        cause: String,
    ) {
        self.queue.clear();
        self.plan = Plan::ApplyKeymap {
            before,
            target,
            stage: KeyStage::ObserveFn,
            cause: Some(cause),
        };
        self.keymap_read();
    }

    fn keymap_stage_done(
        &mut self,
        before: adapter::Snapshot,
        target: adapter::Snapshot,
        stage: KeyStage,
        cause: Option<String>,
    ) {
        match stage {
            KeyStage::Main => match self.keymap_snapshot() {
                Ok(actual) if actual == target => match adapter::from_snapshot(&actual) {
                    Ok(state) => {
                        self.complete(CompletionPayload::Keymap(FeatureResult::Apply(Ok(state))))
                    }
                    Err(error) => self.begin_key_recovery(before, target, error),
                },
                Ok(_) => self.begin_key_recovery(
                    before,
                    target,
                    "Readback does not match the complete intended keymaps".into(),
                ),
                Err(error) => self.begin_key_recovery(before, target, error),
            },
            KeyStage::ObserveFn | KeyStage::ObserveBase => {
                let actual = self.keymap_snapshot().ok();
                let function = stage == KeyStage::ObserveFn;
                let (attempted, original, observed, index) = if function {
                    (
                        &target.function,
                        &before.function,
                        actual.as_ref().map(|value| value.function.as_slice()),
                        0,
                    )
                } else {
                    (
                        &target.base,
                        &before.base,
                        actual.as_ref().map(|value| value.base.as_slice()),
                        before.profile,
                    )
                };
                let slots = match recovery_keymaps::slots_to_restore(observed, attempted, original)
                {
                    Ok(slots) => slots,
                    Err(error) => {
                        self.failed(
                            format!(
                                "{}; recovery selection failed: {error}",
                                cause.unwrap_or_default()
                            ),
                            Recovery::Unverified,
                        );
                        return;
                    }
                };
                for slot in slots {
                    let report =
                        match protocol::single_key_report(function, index, slot, original[slot]) {
                            Ok(report) => report,
                            Err(error) => {
                                self.failed(
                                    format!(
                                        "{}; recovery report failed: {error}",
                                        cause.unwrap_or_default()
                                    ),
                                    Recovery::Unverified,
                                );
                                return;
                            }
                        };
                    self.write(report, 0, 1000);
                }
                let next_stage = if function {
                    KeyStage::RestoreFn
                } else {
                    KeyStage::RestoreBase
                };
                self.plan = Plan::ApplyKeymap {
                    before,
                    target,
                    stage: next_stage,
                    cause,
                };
                if self.queue.is_empty() {
                    self.sequence_done();
                }
            }
            KeyStage::RestoreFn => {
                self.plan = Plan::ApplyKeymap {
                    before,
                    target,
                    stage: KeyStage::ObserveBase,
                    cause,
                };
                self.keymap_read();
            }
            KeyStage::RestoreBase => {
                self.plan = Plan::ApplyKeymap {
                    before,
                    target,
                    stage: KeyStage::Verify,
                    cause,
                };
                self.keymap_read();
            }
            KeyStage::Verify => {
                let original_cause = cause.unwrap_or_else(|| "Keymap apply failed".into());
                match self.keymap_snapshot() {
                    Ok(actual) if actual == before => self.failed(
                        format!("{original_cause}; original keymaps restored and verified"),
                        Recovery::Verified,
                    ),
                    Ok(_) => self.failed(
                        format!("{original_cause}; restored keymaps differ from backup"),
                        Recovery::Failed,
                    ),
                    Err(error) => self.failed(
                        format!("{original_cause}; restoration could not be verified: {error}"),
                        Recovery::Unverified,
                    ),
                }
            }
        }
    }

    fn begin_macro_recovery(
        &mut self,
        slot: String,
        number: u8,
        before: Vec<u8>,
        target: Vec<u8>,
        cause: String,
    ) {
        self.queue.clear();
        let reports = match native_macros::write_reports(number, &before) {
            Ok(reports) => reports,
            Err(error) => {
                self.failed(
                    format!("{cause}; recovery report failed: {error}"),
                    Recovery::Unverified,
                );
                return;
            }
        };
        for (page, report) in reports.into_iter().enumerate() {
            self.write(report, 0, if page == 4 { 2030 } else { 30 });
        }
        self.plan = Plan::ApplyMacro {
            slot,
            number,
            before,
            target,
            stage: MacroStage::Restore,
            cause: Some(cause),
        };
    }

    fn macro_stage_done(
        &mut self,
        slot: String,
        number: u8,
        before: Vec<u8>,
        target: Vec<u8>,
        stage: MacroStage,
        cause: Option<String>,
    ) {
        match stage {
            MacroStage::Main => match self.macro_bytes() {
                Ok(raw) if raw == target => match macro_adapter::from_bytes(&slot, &raw) {
                    Ok(snapshot) => self.complete(CompletionPayload::Macro {
                        slot,
                        result: FeatureResult::Apply(Ok(snapshot)),
                    }),
                    Err(error) => self.begin_macro_recovery(slot, number, before, target, error),
                },
                Ok(_) => self.begin_macro_recovery(
                    slot,
                    number,
                    before,
                    target,
                    "Macro readback mismatch".into(),
                ),
                Err(error) => self.begin_macro_recovery(slot, number, before, target, error),
            },
            MacroStage::Restore => {
                self.plan = Plan::ApplyMacro {
                    slot,
                    number,
                    before,
                    target,
                    stage: MacroStage::Verify,
                    cause,
                };
                self.macro_read(number);
            }
            MacroStage::Verify => {
                let original_cause = cause.unwrap_or_else(|| "Macro apply failed".into());
                match self.macro_bytes() {
                    Ok(raw) if raw == before => self.failed(
                        format!("{original_cause}; original macro restored and verified"),
                        Recovery::Verified,
                    ),
                    Ok(_) => self.failed(
                        format!("{original_cause}; restored macro differs from backup"),
                        Recovery::Failed,
                    ),
                    Err(error) => self.failed(
                        format!("{original_cause}; restoration could not be verified: {error}"),
                        Recovery::Unverified,
                    ),
                }
            }
        }
    }

    fn begin_settings_recovery(
        &mut self,
        before: native_settings::Settings,
        target: native_settings::Settings,
        restore_report: [u8; 64],
        edit: settings::Edit,
        cause: String,
    ) {
        self.queue.clear();
        self.write(restore_report, 0, 500);
        self.plan = Plan::ApplySettings {
            before,
            target,
            restore_report,
            edit,
            stage: SettingsStage::Restore,
            cause: Some(cause),
        };
    }

    fn settings_stage_done(
        &mut self,
        before: native_settings::Settings,
        target: native_settings::Settings,
        restore_report: [u8; 64],
        edit: settings::Edit,
        stage: SettingsStage,
        cause: Option<String>,
    ) {
        match stage {
            SettingsStage::Main => {
                let result = self.settings_native().and_then(|actual| {
                    if actual != target {
                        return Err("Settings readback does not match the complete intended state".into());
                    }
                    let snapshot = settings_adapter::project(&actual);
                    if !matches!(&snapshot.content, settings::Content::Editable(values) if values.get(&edit.id) == Some(&edit.value)) {
                        return Err("Settings readback does not match the requested field".into());
                    }
                    Ok(snapshot)
                });
                match result {
                    Ok(snapshot) => self.complete(CompletionPayload::Settings(
                        FeatureResult::Apply(Ok(snapshot)),
                    )),
                    Err(error) => {
                        self.begin_settings_recovery(before, target, restore_report, edit, error)
                    }
                }
            }
            SettingsStage::Restore => {
                self.plan = Plan::ApplySettings {
                    before,
                    target,
                    restore_report,
                    edit,
                    stage: SettingsStage::Verify,
                    cause,
                };
                self.settings_read();
            }
            SettingsStage::Verify => {
                let original_cause = cause.unwrap_or_else(|| "Settings apply failed".into());
                match self.settings_native() {
                    Ok(actual) if actual == before => self.failed(
                        format!("{original_cause}; original settings restored and verified"),
                        Recovery::Verified,
                    ),
                    Ok(_) => self.failed(
                        format!("{original_cause}; restored settings differ from backup"),
                        Recovery::Failed,
                    ),
                    Err(error) => self.failed(
                        format!("{original_cause}; restoration could not be verified: {error}"),
                        Recovery::Unverified,
                    ),
                }
            }
        }
    }

    fn archive_stage_done(&mut self, mut capture: ArchiveCapture, stage: ArchiveStage) {
        match stage {
            ArchiveStage::Keymap => {
                let result = self.keymap_snapshot();
                match result {
                    Ok(keymaps) if keymaps.firmware == 0x0100 && keymaps.profile == 0 => {
                        capture.keymaps = Some(keymaps);
                        self.plan = Plan::ReadArchive {
                            stage: ArchiveStage::Lighting,
                            capture,
                        };
                        self.lighting_read(ReplyTag::Lighting);
                    }
                    Ok(_) => self.read_failed(
                        "Configuration capture requires firmware 0x0100, profile 0".into(),
                    ),
                    Err(error) => self.read_failed(error),
                }
            }
            ArchiveStage::Lighting => {
                let result = self
                    .replies
                    .lighting
                    .ok_or("Missing lighting response".to_owned())
                    .and_then(|bytes| native_lighting::Lighting::decode(&bytes));
                match result {
                    Ok(lighting) => {
                        capture.lighting = Some(lighting);
                        self.plan = Plan::ReadArchive {
                            stage: ArchiveStage::Settings,
                            capture,
                        };
                        self.settings_read();
                    }
                    Err(error) => self.read_failed(error),
                }
            }
            ArchiveStage::Settings => match self.settings_native() {
                Ok(settings) => {
                    capture.settings = Some(settings);
                    self.plan = Plan::ReadArchive {
                        stage: ArchiveStage::Picture,
                        capture,
                    };
                    self.archive_picture_read();
                }
                Err(error) => self.read_failed(error),
            },
            ArchiveStage::Picture => {
                match native_lighting::user_picture_from_pages(&self.replies.picture) {
                    Ok(picture) => {
                        capture.picture = Some(picture);
                        self.plan = Plan::ReadArchive {
                            stage: ArchiveStage::Macro(0),
                            capture,
                        };
                        self.macro_read(0);
                    }
                    Err(error) => self.read_failed(error),
                }
            }
            ArchiveStage::Macro(slot) => match self.macro_bytes() {
                Ok(raw) => {
                    capture.macros.push(raw);
                    if slot < 49 {
                        let next = slot + 1;
                        self.plan = Plan::ReadArchive {
                            stage: ArchiveStage::Macro(next),
                            capture,
                        };
                        self.macro_read(next);
                    } else {
                        let config = configuration::Configuration {
                            keymaps: capture.keymaps.expect("keymaps captured before macros"),
                            macros: capture.macros,
                            lighting: capture.lighting.expect("lighting captured before macros"),
                            picture: capture.picture.expect("picture captured before macros"),
                            settings: capture.settings.expect("settings captured before macros"),
                        };
                        match archive_adapter::encode(&config) {
                            Ok(archive) => self.complete(CompletionPayload::Archive(
                                FeatureResult::Read(Ok(archive)),
                            )),
                            Err(error) => self.read_failed(error),
                        }
                    }
                }
                Err(error) => self.read_failed(error),
            },
        }
    }

    fn fail_step(&mut self, error: String) {
        let current = self.current.clone();
        if matches!(current, Step::Backup(_)) {
            self.failed(
                format!("Backup storage failed before any setter: {error}"),
                Recovery::NotAttempted,
            );
            return;
        }
        let plan = std::mem::replace(&mut self.plan, Plan::Terminal);
        match plan {
            Plan::ReadKeymap
            | Plan::ReadLighting
            | Plan::ReadSettings
            | Plan::ReadArchive { .. }
            | Plan::ReadPicture
            | Plan::ReadMacro { .. }
            | Plan::ReadCatalog { .. } => self.read_failed(error),
            Plan::ApplyLighting { .. } | Plan::ApplyPicture { .. } => self.failed(
                format!("Transport outcome is unknown: {error}; backup retained"),
                Recovery::Unverified,
            ),
            Plan::ApplyKeymap {
                before,
                target,
                stage: KeyStage::Main,
                ..
            } => self.begin_key_recovery(before, target, error),
            Plan::ApplyKeymap {
                before,
                target,
                stage,
                cause,
            } if matches!(stage, KeyStage::ObserveFn | KeyStage::ObserveBase) => {
                self.plan = Plan::ApplyKeymap {
                    before,
                    target,
                    stage,
                    cause,
                };
                self.queue.clear();
                self.sequence_done();
            }
            Plan::ApplyKeymap { cause, .. } => self.failed(
                format!("{}; recovery failed: {error}", cause.unwrap_or_default()),
                Recovery::Unverified,
            ),
            Plan::ApplyMacro {
                slot,
                number,
                before,
                target,
                stage: MacroStage::Main,
                ..
            } => self.begin_macro_recovery(slot, number, before, target, error),
            Plan::ApplyMacro { cause, .. } => self.failed(
                format!("{}; recovery failed: {error}", cause.unwrap_or_default()),
                Recovery::Unverified,
            ),
            Plan::ApplySettings {
                before,
                target,
                restore_report,
                edit,
                stage: SettingsStage::Main,
                ..
            } => self.begin_settings_recovery(before, target, restore_report, edit, error),
            Plan::ApplySettings { cause, .. } => self.failed(
                format!("{}; recovery failed: {error}", cause.unwrap_or_default()),
                Recovery::Unverified,
            ),
            Plan::Terminal => self.read_failed(error),
        }
    }

    fn effect_error(&mut self, error: Value) {
        let (message, attempted) = match error {
            Value::String(message) => (message, true),
            Value::Object(fields) => {
                let message = fields
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Browser effect failed")
                    .to_owned();
                let attempted = fields
                    .get("setterAttempted")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                (message, attempted)
            }
            _ => ("Browser effect failed".into(), true),
        };
        if matches!(self.current, Step::Write { .. }) {
            self.setter_attempted |= attempted;
            if !self.setter_attempted {
                self.failed(
                    format!("Setter was not attempted: {message}"),
                    Recovery::NotAttempted,
                );
                return;
            }
        }
        self.fail_step(message);
    }

    #[wasm_bindgen(constructor)]
    pub fn new(command_json: &str) -> Result<BrowserOperation, JsValue> {
        let command: Command = serde_json::from_str(command_json)
            .map_err(|error| JsValue::from_str(&format!("Invalid command JSON: {error}")))?;
        let mut operation = BrowserOperation {
            command,
            plan: Plan::Terminal,
            queue: VecDeque::new(),
            current: Step::Complete(Completion {
                generation: 0,
                operation: 0,
                payload: CompletionPayload::ReadMacroCatalog {
                    result: Err("Uninitialized operation".into()),
                },
            }),
            replies: Replies::default(),
            setter_attempted: false,
            yield_between_catalog_slots: false,
        };
        operation.start();
        operation.next();
        Ok(operation)
    }

    pub fn step(&self) -> String {
        self.step_value().to_string()
    }

    /// True only after a complete macro slot and before beginning the next one.
    pub fn can_yield(&self) -> bool {
        self.yield_between_catalog_slots
    }

    pub fn advance(&mut self, response_json: &str) -> Result<String, JsValue> {
        if matches!(self.current, Step::Complete(_)) {
            return Ok(self.step());
        }
        let response: Result<Option<Vec<u8>>, Value> = serde_json::from_str(response_json)
            .map_err(|error| JsValue::from_str(&format!("Invalid effect result: {error}")))?;
        let current = self.current.clone();
        self.yield_between_catalog_slots = false;
        match response {
            Err(error) => self.effect_error(error),
            Ok(value) => match current {
                Step::Backup(_) | Step::Write { .. } => {
                    if matches!(current, Step::Write { .. }) {
                        self.setter_attempted = true;
                    }
                    if value.is_some() {
                        self.fail_step("Effect acknowledgement must be null".into());
                    }
                }
                Step::Exchange { tag, .. } => match value {
                    Some(bytes) => {
                        if let Err(error) = self.accept_reply(tag, &bytes) {
                            self.fail_step(error);
                        }
                    }
                    None => self.fail_step("Feature exchange requires a 64-byte reply".into()),
                },
                Step::Complete(_) => unreachable!(),
            },
        }
        self.next();
        Ok(self.step())
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;

impl BrowserOperation {
    fn step_value(&self) -> Value {
        match &self.current {
            Step::Backup(record) => json!({"kind":"backup","record":record}),
            Step::Exchange { report, .. } => {
                json!({"kind":"exchange","report":report,"delay_ms":30})
            }
            Step::Write {
                report,
                before_ms,
                after_ms,
            } => json!({"kind":"write","report":report,"before_ms":before_ms,"after_ms":after_ms}),
            Step::Complete(completion) => json!({"kind":"complete","completion":completion}),
        }
    }

    fn complete(&mut self, payload: CompletionPayload) {
        self.plan = Plan::Terminal;
        self.queue.clear();
        self.queue
            .push_back(Step::Complete(self.command.clone().map(|_| payload)));
    }

    fn failed(&mut self, message: String, recovery: Recovery) {
        let failure = ApplyFailure { message, recovery };
        let payload = match &self.command.payload {
            CommandPayload::Keymap(_) => {
                CompletionPayload::Keymap(FeatureResult::Apply(Err(failure)))
            }
            CommandPayload::Macro(FeatureCommand::Apply { expected, .. }) => {
                CompletionPayload::Macro {
                    slot: expected.slot.clone(),
                    result: FeatureResult::Apply(Err(failure)),
                }
            }
            CommandPayload::Lighting(_) => {
                CompletionPayload::Lighting(FeatureResult::Apply(Err(failure)))
            }
            CommandPayload::Picture(_) => {
                CompletionPayload::Picture(FeatureResult::Apply(Err(failure)))
            }
            CommandPayload::Settings(_) => {
                CompletionPayload::Settings(FeatureResult::Apply(Err(failure)))
            }
            _ => CompletionPayload::ReadMacroCatalog {
                result: Err(failure.message),
            },
        };
        self.complete(payload);
    }

    fn read_failed(&mut self, message: String) {
        let payload = match &self.command.payload {
            CommandPayload::Keymap(_) => {
                CompletionPayload::Keymap(FeatureResult::Read(Err(message)))
            }
            CommandPayload::Macro(FeatureCommand::Read(slot)) => CompletionPayload::Macro {
                slot: slot.clone(),
                result: FeatureResult::Read(Err(message)),
            },
            CommandPayload::Lighting(_) => {
                CompletionPayload::Lighting(FeatureResult::Read(Err(message)))
            }
            CommandPayload::Picture(_) => {
                CompletionPayload::Picture(FeatureResult::Read(Err(message)))
            }
            CommandPayload::Settings(_) => {
                CompletionPayload::Settings(FeatureResult::Read(Err(message)))
            }
            CommandPayload::Archive(FeatureCommand::Read(())) => {
                CompletionPayload::Archive(FeatureResult::Read(Err(message)))
            }
            CommandPayload::ReadMacroCatalog { .. } => CompletionPayload::ReadMacroCatalog {
                result: Err(message),
            },
            _ => CompletionPayload::ReadMacroCatalog {
                result: Err(message),
            },
        };
        self.complete(payload);
    }

    fn unsupported(&mut self) {
        let message = "This browser preview does not support that feature".to_owned();
        let payload = match &self.command.payload {
            CommandPayload::Settings(FeatureCommand::Read(_)) => {
                CompletionPayload::Settings(FeatureResult::Read(Err(message)))
            }
            CommandPayload::Settings(FeatureCommand::Apply { .. }) => {
                CompletionPayload::Settings(FeatureResult::Apply(Err(ApplyFailure {
                    message,
                    recovery: Recovery::NotAttempted,
                })))
            }
            CommandPayload::Archive(FeatureCommand::Read(_)) => {
                CompletionPayload::Archive(FeatureResult::Read(Err(message)))
            }
            CommandPayload::Archive(FeatureCommand::Apply { .. }) => {
                CompletionPayload::Archive(FeatureResult::Apply(Err(ApplyFailure {
                    message,
                    recovery: Recovery::NotAttempted,
                })))
            }
            CommandPayload::ReviewArchive { .. } => CompletionPayload::ReviewArchive {
                result: Err(message),
            },
            _ => CompletionPayload::ReadMacroCatalog {
                result: Err(message),
            },
        };
        self.complete(payload);
    }

    fn start(&mut self) {
        match self.command.payload.clone() {
            CommandPayload::Keymap(FeatureCommand::Read(())) => {
                self.plan = Plan::ReadKeymap;
                self.keymap_read();
            }
            CommandPayload::Lighting(FeatureCommand::Read(())) => {
                self.plan = Plan::ReadLighting;
                self.lighting_read(ReplyTag::Lighting);
            }
            CommandPayload::Settings(FeatureCommand::Read(())) => {
                self.plan = Plan::ReadSettings;
                self.settings_read();
            }
            CommandPayload::Archive(FeatureCommand::Read(())) => {
                self.plan = Plan::ReadArchive {
                    stage: ArchiveStage::Keymap,
                    capture: ArchiveCapture::default(),
                };
                self.keymap_read();
            }
            CommandPayload::Picture(FeatureCommand::Read(())) => {
                self.plan = Plan::ReadPicture;
                self.picture_read();
            }
            CommandPayload::Macro(FeatureCommand::Read(slot)) => {
                match macro_adapter::slot_number(&slot) {
                    Ok(number) => {
                        self.plan = Plan::ReadMacro { slot };
                        self.macro_read(number);
                    }
                    Err(error) => self.read_failed(error),
                }
            }
            CommandPayload::ReadMacroCatalog { slots } => {
                let numbers = slots
                    .iter()
                    .map(|slot| macro_adapter::slot_number(slot))
                    .collect::<Result<Vec<_>, _>>();
                match numbers {
                    Ok(numbers) if numbers.is_empty() => {
                        self.complete(CompletionPayload::ReadMacroCatalog {
                            result: Ok(Vec::new()),
                        })
                    }
                    Ok(numbers) => {
                        self.plan = Plan::ReadCatalog {
                            slots,
                            next: 0,
                            results: Vec::new(),
                        };
                        self.macro_read(numbers[0]);
                    }
                    Err(error) => self.read_failed(error),
                }
            }
            CommandPayload::Keymap(FeatureCommand::Apply { expected, desired }) => {
                self.start_keymap(expected, desired)
            }
            CommandPayload::Macro(FeatureCommand::Apply { expected, desired }) => {
                self.start_macro(expected, desired)
            }
            CommandPayload::Lighting(FeatureCommand::Apply { expected, desired }) => {
                self.start_lighting(expected, desired)
            }
            CommandPayload::Settings(FeatureCommand::Apply { expected, desired }) => {
                self.start_settings(expected, desired)
            }
            CommandPayload::Picture(FeatureCommand::Apply { expected, desired }) => {
                self.start_picture(expected, desired)
            }
            _ => self.unsupported(),
        }
    }

    fn exchange(&mut self, report: [u8; 64], tag: ReplyTag) {
        self.queue.push_back(Step::Exchange {
            report: report.to_vec(),
            tag,
        });
    }
    fn write(&mut self, report: [u8; 64], before_ms: u32, after_ms: u32) {
        self.queue.push_back(Step::Write {
            report: report.to_vec(),
            before_ms,
            after_ms,
        });
    }
    fn keymap_read(&mut self) {
        self.replies = Replies::default();
        self.exchange(protocol::read_request(0x80, 0, 0), ReplyTag::Version);
        self.exchange(protocol::read_request(0x85, 0, 0), ReplyTag::Profile);
    }
    fn lighting_read(&mut self, tag: ReplyTag) {
        self.replies = Replies::default();
        self.exchange(native_lighting::read_request(), tag);
    }
    fn picture_read(&mut self) {
        self.replies = Replies::default();
        self.exchange(native_lighting::read_request(), ReplyTag::PictureContext);
    }
    fn archive_picture_read(&mut self) {
        self.replies = Replies::default();
        for report in native_lighting::user_picture_read_requests() {
            self.exchange(report, ReplyTag::PicturePage);
        }
    }
    fn macro_read(&mut self, number: u8) {
        self.replies = Replies::default();
        for page in 0..4 {
            match native_macros::read_request(number, page) {
                Ok(report) => self.exchange(report, ReplyTag::MacroPage),
                Err(error) => {
                    self.read_failed(error);
                    return;
                }
            }
        }
    }
    fn settings_read(&mut self) {
        self.replies = Replies::default();
        for (opcode, report) in [
            native_settings::DEBOUNCE_READ,
            native_settings::AUTO_OS_READ,
            native_settings::SLEEP_READ,
            native_settings::OPTIONS_READ,
        ]
        .into_iter()
        .zip(native_settings::read_requests())
        {
            self.exchange(report, ReplyTag::Setting(opcode));
        }
    }

    fn accept_reply(&mut self, tag: ReplyTag, bytes: &[u8]) -> Result<(), String> {
        let reply: [u8; 64] = bytes
            .try_into()
            .map_err(|_| "Expected exactly 64 feature bytes".to_owned())?;
        match tag {
            ReplyTag::Version => {
                if reply[0] != 0x80 {
                    return Err("Unrelated version response".into());
                }
                self.replies.version = Some(reply);
            }
            ReplyTag::Profile => {
                if reply[0] != 0x85 {
                    return Err("Unrelated profile response".into());
                }
                self.replies.profile = Some(reply);
                for page in 0..8 {
                    self.exchange(protocol::read_request(0x89, reply[1], page), ReplyTag::Base);
                }
                for page in 0..8 {
                    self.exchange(protocol::read_request(0x90, 0, page), ReplyTag::Function);
                }
            }
            ReplyTag::Base => self.replies.base.push(reply),
            ReplyTag::Function => self.replies.function.push(reply),
            ReplyTag::Lighting => {
                if reply[0] != native_lighting::LED_READ_COMMAND {
                    return Err("Unrelated lighting response".into());
                }
                self.replies.lighting = Some(reply);
            }
            ReplyTag::PictureContext => {
                if reply[0] != native_lighting::LED_READ_COMMAND {
                    return Err("Unrelated lighting context response".into());
                }
                self.replies.context = Some(reply);
                for report in native_lighting::user_picture_read_requests() {
                    self.exchange(report, ReplyTag::PicturePage);
                }
            }
            ReplyTag::PicturePage => self.replies.picture.push(reply),
            ReplyTag::MacroPage => self.replies.macro_pages.push(reply),
            ReplyTag::Setting(opcode) => {
                if reply[0] != opcode {
                    return Err(format!(
                        "Unrelated settings response: expected 0x{opcode:02x}, got 0x{:02x}",
                        reply[0]
                    ));
                }
                self.replies.settings.push(reply);
            }
        }
        Ok(())
    }

    fn keymap_snapshot(&self) -> Result<adapter::Snapshot, String> {
        let version = self.replies.version.ok_or("Missing version response")?;
        let profile = self.replies.profile.ok_or("Missing profile response")?;
        Ok(adapter::Snapshot {
            format_version: 1,
            firmware: u16::from_le_bytes([version[1], version[2]]),
            profile: profile[1],
            base: protocol::matrix_from_pages(&self.replies.base)?,
            function: protocol::matrix_from_pages(&self.replies.function)?,
        })
    }

    fn macro_bytes(&self) -> Result<Vec<u8>, String> {
        if self.replies.macro_pages.len() != 4 {
            return Err("Incomplete macro reply".into());
        }
        Ok(self
            .replies
            .macro_pages
            .iter()
            .flat_map(|page| page.iter().copied())
            .collect())
    }

    fn settings_native(&self) -> Result<native_settings::Settings, String> {
        let [debounce, auto_os, sleep, options]: &[[u8; 64]; 4] = self
            .replies
            .settings
            .as_slice()
            .try_into()
            .map_err(|_| "Incomplete settings reply".to_owned())?;
        native_settings::Settings::decode(debounce, auto_os, sleep, options)
    }

    fn next(&mut self) {
        if let Some(step) = self.queue.pop_front() {
            self.current = step;
            return;
        }
        self.sequence_done();
        self.current = self
            .queue
            .pop_front()
            .expect("sequence must produce a step");
    }

    fn sequence_done(&mut self) {
        let plan = std::mem::replace(&mut self.plan, Plan::Terminal);
        match plan {
            Plan::ReadKeymap => match self
                .keymap_snapshot()
                .and_then(|raw| adapter::from_snapshot(&raw))
            {
                Ok(state) => {
                    self.complete(CompletionPayload::Keymap(FeatureResult::Read(Ok(state))))
                }
                Err(error) => self.read_failed(error),
            },
            Plan::ReadLighting => match self
                .replies
                .lighting
                .ok_or("Missing lighting response".to_owned())
                .and_then(|bytes| lighting_adapter::from_bytes(&bytes))
            {
                Ok(snapshot) => self.complete(CompletionPayload::Lighting(FeatureResult::Read(
                    Ok(snapshot),
                ))),
                Err(error) => self.read_failed(error),
            },
            Plan::ReadSettings => match self.settings_native() {
                Ok(native) => self.complete(CompletionPayload::Settings(FeatureResult::Read(Ok(
                    settings_adapter::project(&native),
                )))),
                Err(error) => self.read_failed(error),
            },
            Plan::ReadArchive { stage, capture } => self.archive_stage_done(capture, stage),
            Plan::ReadPicture => {
                let result = self
                    .replies
                    .context
                    .ok_or("Missing picture context".to_owned())
                    .and_then(|raw| native_lighting::Lighting::decode(&raw))
                    .and_then(|lighting| {
                        native_lighting::user_picture_from_pages(&self.replies.picture).and_then(
                            |colors| picture_adapter::project(&colors, lighting.picture_context()),
                        )
                    });
                match result {
                    Ok(snapshot) => self.complete(CompletionPayload::Picture(FeatureResult::Read(
                        Ok(snapshot),
                    ))),
                    Err(error) => self.read_failed(error),
                }
            }
            Plan::ReadMacro { slot, .. } => match self
                .macro_bytes()
                .and_then(|raw| macro_adapter::from_bytes(&slot, &raw))
            {
                Ok(snapshot) => self.complete(CompletionPayload::Macro {
                    slot,
                    result: FeatureResult::Read(Ok(snapshot)),
                }),
                Err(error) => self.read_failed(error),
            },
            Plan::ReadCatalog {
                slots,
                next,
                mut results,
            } => {
                let outcome = self
                    .macro_bytes()
                    .and_then(|raw| macro_adapter::from_bytes(&slots[next], &raw));
                match outcome {
                    Ok(snapshot) => {
                        results.push(snapshot);
                        if next + 1 == slots.len() {
                            self.complete(CompletionPayload::ReadMacroCatalog {
                                result: Ok(results),
                            });
                        } else {
                            let next = next + 1;
                            let number =
                                macro_adapter::slot_number(&slots[next]).expect("validated slots");
                            self.plan = Plan::ReadCatalog {
                                slots,
                                next,
                                results,
                            };
                            self.macro_read(number);
                            self.yield_between_catalog_slots = true;
                        }
                    }
                    Err(error) => self.read_failed(error),
                }
            }
            Plan::ApplyKeymap {
                before,
                target,
                stage,
                cause,
            } => self.keymap_stage_done(before, target, stage, cause),
            Plan::ApplyMacro {
                slot,
                number,
                before,
                target,
                stage,
                cause,
            } => self.macro_stage_done(slot, number, before, target, stage, cause),
            Plan::ApplySettings {
                before,
                target,
                restore_report,
                edit,
                stage,
                cause,
            } => self.settings_stage_done(before, target, restore_report, edit, stage, cause),
            Plan::ApplyLighting { submitted } => self.complete(CompletionPayload::Lighting(
                FeatureResult::Apply(Ok(submitted)),
            )),
            Plan::ApplyPicture { submitted } => self.complete(CompletionPayload::Picture(
                FeatureResult::Apply(Ok(submitted)),
            )),
            Plan::Terminal => unreachable!("terminal has no further sequence"),
        }
    }
}
