//! Browser presentation boundary for the shared core session. The JSON protocol
//! carries core-owned commands and completions without interpreting device bytes.
use byakko_core::{
    contract::{Command, Completion},
    editor::{Editor, Feature, Status},
    model::macros,
    session::{Connection, Outcome, Session},
};
use byakko_protocol::nia87::{adapter, lighting_adapter, macro_adapter, picture_adapter};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct BrowserSession {
    session: Session,
}

#[wasm_bindgen]
impl BrowserSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<BrowserSession, JsValue> {
        let session = Session::new(adapter::descriptor())
            .and_then(|session| session.with_lighting(lighting_adapter::capabilities()))
            .and_then(|session| session.with_picture(picture_adapter::capabilities()))
            .and_then(|session| session.with_macros(macro_adapter::capabilities()))
            .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self { session })
    }

    /// Intent JSON uses {type, feature?, change?, slot?, layer?, key?, binding?}.
    /// Changes use the corresponding byakko-core serde enum representation.
    /// Every response contains the current view, even when an intent is rejected.
    pub fn dispatch(&mut self, input: &str) -> String {
        let result = serde_json::from_str::<Value>(input)
            .map_err(|error| error.to_string())
            .and_then(|intent| self.dispatch_value(&intent));
        self.response(result)
    }

    /// Completion JSON is byakko_core::contract::Completion. Correlation and
    /// continuation workflows are checked by Session::accept.
    pub fn accept(&mut self, input: &str) -> String {
        let result = serde_json::from_str::<Completion>(input)
            .map_err(|error| error.to_string())
            .map(|completion| outcome(self.session.accept(completion)));
        self.response(result)
    }

    pub fn view(&self) -> String {
        self.view_value().to_string()
    }
}

struct ResultValue {
    command: Option<Command>,
    outcome: Option<Value>,
}

impl ResultValue {
    fn command(command: Command) -> Self {
        Self {
            command: Some(command),
            outcome: None,
        }
    }

    fn done() -> Self {
        Self {
            command: None,
            outcome: None,
        }
    }

    fn note(value: Value) -> Self {
        Self {
            command: None,
            outcome: Some(value),
        }
    }
}

impl BrowserSession {
    fn dispatch_value(&mut self, intent: &Value) -> Result<ResultValue, String> {
        let kind = required_str(intent, "type")?;
        let feature = || required_str(intent, "feature");
        let command = match kind {
            "connect" => {
                let generation = self.session.connect()?;
                return Ok(ResultValue::note(
                    json!({"kind": "connected", "generation": generation}),
                ));
            }
            "disconnect" => {
                self.session.disconnect()?;
                return Ok(ResultValue::note(json!({"kind": "disconnected"})));
            }
            "refreshNext" => {
                return Ok(ResultValue {
                    command: self.session.refresh_next()?,
                    outcome: None,
                });
            }
            "read" => match feature()? {
                "keymap" => self.session.read()?,
                "lighting" => self.session.read_lighting()?,
                "picture" => self.session.read_picture()?,
                "macro" => self.session.read_macro()?,
                "settings" => self.session.read_settings()?,
                _ => return Err("Unknown feature".into()),
            },
            "edit" => {
                let change = intent.get("change").ok_or("Missing change")?.clone();
                match feature()? {
                    "keymap" => self.session.edit(parse(change)?)?,
                    "lighting" => self.session.edit_lighting(parse(change)?)?,
                    "picture" => self.session.edit_picture(parse(change)?)?,
                    "macro" => self.session.edit_macro(parse(change)?)?,
                    "settings" => self.session.edit_settings(parse(change)?)?,
                    _ => return Err("Unknown feature".into()),
                }
                return Ok(ResultValue::done());
            }
            "revert" => {
                match feature()? {
                    "keymap" => self.session.revert()?,
                    "lighting" => self.session.revert_lighting()?,
                    "picture" => self.session.revert_picture()?,
                    "macro" => self.session.revert_macro()?,
                    "settings" => self.session.revert_settings()?,
                    _ => return Err("Unknown feature".into()),
                }
                return Ok(ResultValue::done());
            }
            "apply" => match feature()? {
                "keymap" => self.session.save()?,
                "lighting" => self.session.save_lighting()?,
                "picture" => self.session.save_picture()?,
                "macro" => self.session.save_macro()?,
                "settings" => self.session.save_settings()?,
                _ => return Err("Unknown feature".into()),
            },
            "selectMacro" => {
                self.session.select_macro(required_str(intent, "slot")?)?;
                return Ok(ResultValue::done());
            }
            "initializeMacro" => {
                let editor = self.session.macros().ok_or("Macros are not supported")?;
                let draft = editor.draft().ok_or("Read this macro slot first")?;
                if editor.status() != &Status::Ready || editor.dirty() || !draft.events.is_empty() {
                    return Err("Only a clean, empty macro can be initialized".into());
                }
                let count = draft.repeat_count;
                let range = &editor.capabilities().editable_repeat_counts;
                if !range.contains(&count) {
                    self.session
                        .edit_macro(macros::Edit::Repeat(*range.start()))?;
                }
                return Ok(ResultValue::done());
            }
            "discoverMacros" => self.session.request_macro_catalog()?,
            "macroCandidate" => {
                return Ok(ResultValue::note(
                    json!({"kind": "macroCandidate", "slot": self.session.macro_candidate()?}),
                ));
            }
            "saveAndAssignMacro" => self.session.save_and_assign_macro(
                required_str(intent, "layer")?,
                required_str(intent, "key")?,
                required_str(intent, "binding")?,
            )?,
            "preparePicture" => {
                let command = self.session.prepare_picture()?;
                return Ok(ResultValue {
                    outcome: command
                        .is_none()
                        .then(|| json!({"kind": "picturePrepared"})),
                    command,
                });
            }
            "importMacroDocument" => {
                let document = parse_field(intent, "document")?;
                let metadata = self.session.stage_macro_document(&document)?;
                return Ok(ResultValue::note(json!({
                    "kind": "macroDocumentImported",
                    "name": metadata.name,
                    "binding": metadata.binding,
                })));
            }
            "exportMacroDocument" => {
                let document = self.session.export_macro_document(
                    required_str(intent, "name")?.to_owned(),
                    intent
                        .get("binding")
                        .map(|value| parse(value.clone()))
                        .transpose()?
                        .flatten(),
                )?;
                return Ok(ResultValue::note(
                    json!({"kind": "macroDocumentExported", "document": document}),
                ));
            }
            "recordStart" => {
                self.session
                    .start_recording(parse_field(intent, "policy")?)?;
                return Ok(ResultValue::done());
            }
            "recordInput" => {
                let action = parse_field(intent, "action")?;
                let at = required_u64(intent, "nowMs")?;
                let transition = self.session.record_input(action, at)?;
                return Ok(ResultValue::note(
                    json!({"kind": format!("{transition:?}")}),
                ));
            }
            "recordStop" => {
                let result = self
                    .session
                    .stop_recording(required_u64(intent, "nowMs")?)?;
                return Ok(ResultValue::note(json!({"kind": format!("{result:?}")})));
            }
            _ => return Err("Unknown intent type".into()),
        };
        Ok(ResultValue::command(command))
    }

    fn response(&self, result: Result<ResultValue, String>) -> String {
        match result {
            Ok(result) => json!({
                "ok": true,
                "command": result.command,
                "outcome": result.outcome,
                "view": self.view_value(),
            })
            .to_string(),
            Err(error) => json!({
                "ok": false,
                "error": error,
                "command": null,
                "view": self.view_value(),
            })
            .to_string(),
        }
    }

    fn view_value(&self) -> Value {
        let session = &self.session;
        let connection = match session.connection() {
            Connection::Disconnected => json!({"kind": "disconnected"}),
            Connection::Connected { generation } => {
                json!({"kind": "connected", "generation": generation})
            }
        };
        let idle = !session.busy() && !session.recording() && session.host().is_idle();
        let can_read = matches!(session.connection(), Connection::Connected { .. }) && idle;
        let can_edit =
            !session.blocks_editing() && !session.recording() && session.host().is_idle();
        let macros = session.macros().map(|editor| {
            let library = session.macro_library().expect("macro editor owns library");
            let occupancy = library
                .slots()
                .iter()
                .map(|(slot, value)| (slot.clone(), json!(format!("{value:?}"))))
                .collect::<serde_json::Map<_, _>>();
            json!({
                "editor": feature_value(editor, can_read, can_edit, idle),
                "capabilities": editor.capabilities(),
                "slot": editor.slot(),
                "occupancy": occupancy,
                "discoveryBusy": library.scanning(),
                "discoveryError": library.error(),
            })
        });
        json!({
            "connection": connection,
            "busy": session.busy(),
            "blocksEditing": session.blocks_editing(),
            "recording": session.recording(),
            "requiresManualRead": session.requires_manual_read(),
            "descriptor": session.descriptor(),
            "keymap": feature_value(session.keymap(), can_read, can_edit, idle),
            "lighting": session.lighting().map(|editor| json!({
                "editor": feature_value(editor, can_read, can_edit, idle),
                "capabilities": editor.capabilities()
            })),
            "picture": session.picture().map(|editor| json!({
                "editor": feature_value(editor, can_read, can_edit, idle),
                "capabilities": editor.capabilities()
            })),
            "macros": macros,
            "settings": session.settings().map(|editor| feature_value(editor, can_read, can_edit, idle)),
        })
    }
}

fn feature_value<F: Feature>(
    editor: &Editor<F>,
    can_read: bool,
    can_edit: bool,
    idle: bool,
) -> Value
where
    F::Snapshot: Serialize,
    F::Value: Serialize,
{
    let ready = editor.status() == &Status::Ready;
    let has_draft = editor.draft().is_some();
    let applying = editor.submitted().is_some();
    json!({
        "baseline": editor.baseline(),
        "draft": editor.draft(),
        "submitted": editor.submitted(),
        "status": editor.status(),
        "dirty": editor.dirty(),
        "canRead": can_read,
        "canEdit": ready && has_draft && can_edit,
        "canApply": ready && editor.dirty() && can_read && !applying,
        "canRevert": editor.baseline().is_some() && !applying && idle,
    })
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing or invalid {key}"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Missing or invalid {key}"))
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn parse_field<T: DeserializeOwned>(value: &Value, key: &str) -> Result<T, String> {
    parse(
        value
            .get(key)
            .ok_or_else(|| format!("Missing {key}"))?
            .clone(),
    )
}

fn outcome(outcome: Outcome) -> ResultValue {
    let (kind, command, detail) = match outcome {
        Outcome::Ignored => ("ignored", None, Value::Null),
        Outcome::Loaded => ("loaded", None, Value::Null),
        Outcome::Saved => ("saved", None, Value::Null),
        Outcome::Conflict => ("conflict", None, Value::Null),
        Outcome::Failed(problem) => ("failed", None, json!(problem)),
        Outcome::MacroLoaded => ("macroLoaded", None, Value::Null),
        Outcome::MacroSaved => ("macroSaved", None, Value::Null),
        Outcome::LightingLoaded => ("lightingLoaded", None, Value::Null),
        Outcome::LightingSaved => ("lightingSaved", None, Value::Null),
        Outcome::PictureLoaded => ("pictureLoaded", None, Value::Null),
        Outcome::PictureSaved => ("pictureSaved", None, Value::Null),
        Outcome::SettingsLoaded => ("settingsLoaded", None, Value::Null),
        Outcome::SettingsSaved => ("settingsSaved", None, Value::Null),
        Outcome::ArchiveCaptured => ("archiveCaptured", None, Value::Null),
        Outcome::ArchiveCaptureFailed(problem) => {
            ("archiveCaptureFailed", None, json!(format!("{problem:?}")))
        }
        Outcome::PicturePreparationFailed {
            lighting_applied,
            problem,
        } => (
            "picturePreparationFailed",
            None,
            json!({"lightingApplied": lighting_applied, "problem": format!("{problem:?}")}),
        ),
        Outcome::CatalogLoaded => ("catalogLoaded", None, Value::Null),
        Outcome::CatalogFailed(problem) => ("catalogFailed", None, json!(problem)),
        Outcome::Continue(command) => ("continue", Some(command), Value::Null),
        Outcome::AssignmentSucceeded { macro_saved } => (
            "assignmentSucceeded",
            None,
            json!({"macroSaved": macro_saved}),
        ),
        Outcome::AssignmentFailed {
            macro_saved,
            problem,
        } => (
            "assignmentFailed",
            None,
            json!({"macroSaved": macro_saved, "problem": format!("{problem:?}")}),
        ),
    };
    ResultValue {
        command,
        outcome: Some(json!({"kind": kind, "detail": detail})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{
        contract::ApplyFailure,
        model::keymap::{self, Action, State},
    };
    use std::collections::BTreeMap;

    fn send(browser: &mut BrowserSession, intent: Value) -> Value {
        serde_json::from_str(&browser.dispatch(&intent.to_string())).unwrap()
    }

    fn load_keymap(browser: &mut BrowserSession) -> Value {
        let connected = send(browser, json!({"type": "connect"}));
        assert_eq!(connected["ok"], true);
        let read = send(browser, json!({"type": "read", "feature": "keymap"}));
        let command = &read["command"];
        let descriptor = browser.session.descriptor();
        let bindings = descriptor
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    descriptor
                        .keys
                        .iter()
                        .map(|key| (key.id.clone(), Action::Key(4)))
                        .collect(),
                )
            })
            .collect::<BTreeMap<_, BTreeMap<_, _>>>();
        let state = State {
            revision: vec![9],
            bindings,
        };
        let completion = json!({
            "generation": command["generation"],
            "operation": command["operation"],
            "payload": {"Keymap": {"Read": {"Ok": state}}}
        });
        serde_json::from_str(&browser.accept(&completion.to_string())).unwrap()
    }

    #[test]
    fn invalid_intent_keeps_shared_session_state() {
        let mut browser = BrowserSession::new().unwrap();
        let before = browser.view();
        let rejected: Value = serde_json::from_str(
            &browser.dispatch(r#"{"type":"edit","feature":"keymap","change":{}}"#),
        )
        .unwrap();
        assert_eq!(rejected["ok"], false);
        assert_eq!(browser.view(), before);
    }

    #[test]
    fn stale_completion_is_ignored_by_core() {
        let mut browser = BrowserSession::new().unwrap();
        let connected: Value =
            serde_json::from_str(&browser.dispatch(r#"{"type":"connect"}"#)).unwrap();
        let generation = connected["outcome"]["generation"].as_u64().unwrap();
        let completion = json!({
            "generation": generation,
            "operation": 999,
            "payload": {"Keymap": {"Read": {"Err": "stale"}}}
        });
        let result: Value = serde_json::from_str(&browser.accept(&completion.to_string())).unwrap();
        assert_eq!(result["outcome"]["kind"], "ignored");
    }

    #[test]
    fn protected_position_rejection_retains_draft() {
        let mut browser = BrowserSession::new().unwrap();
        assert_eq!(load_keymap(&mut browser)["outcome"]["kind"], "loaded");
        let protected = browser
            .session
            .descriptor()
            .layers
            .iter()
            .find_map(|layer| {
                layer
                    .read_only_keys
                    .first()
                    .map(|key| (layer.id.clone(), key.clone()))
            })
            .unwrap();
        let before = browser.view();
        let rejected = send(
            &mut browser,
            json!({
                "type": "edit", "feature": "keymap",
                "change": {"layer": protected.0, "key": protected.1, "action": {"Key": 5}}
            }),
        );
        assert_eq!(rejected["ok"], false);
        assert_eq!(browser.view(), before);
    }

    #[test]
    fn failed_apply_retains_submitted_draft_and_requires_read() {
        let mut browser = BrowserSession::new().unwrap();
        assert_eq!(load_keymap(&mut browser)["outcome"]["kind"], "loaded");
        let descriptor = browser.session.descriptor();
        let (layer, key) = descriptor
            .layers
            .iter()
            .find_map(|layer| {
                descriptor
                    .keys
                    .iter()
                    .find(|key| descriptor.key_is_writable(&layer.id, &key.id))
                    .map(|key| (layer.id.clone(), key.id.clone()))
            })
            .unwrap();
        let action = descriptor
            .actions
            .iter()
            .map(|choice| &choice.action)
            .find(|action| !matches!(action, Action::Key(4) | Action::Opaque { .. }))
            .unwrap()
            .clone();
        let edited = send(
            &mut browser,
            json!({
                "type": "edit", "feature": "keymap",
                "change": {"layer": layer, "key": key, "action": action}
            }),
        );
        assert_eq!(edited["ok"], true);
        assert_eq!(edited["view"]["keymap"]["dirty"], true);
        let apply = send(&mut browser, json!({"type": "apply", "feature": "keymap"}));
        assert_eq!(apply["ok"], true);
        let command = &apply["command"];
        let completion = json!({
            "generation": command["generation"],
            "operation": command["operation"],
            "payload": {"Keymap": {"Apply": {"Err": ApplyFailure {
                message: "simulated write failure".into(),
                recovery: byakko_core::contract::Recovery::Verified,
            }}}}
        });
        let result: Value = serde_json::from_str(&browser.accept(&completion.to_string())).unwrap();
        assert_eq!(result["outcome"]["kind"], "failed");
        assert_eq!(result["view"]["keymap"]["dirty"], true);
        assert!(result["view"]["keymap"]["submitted"].is_null());
        assert!(result["view"]["keymap"]["status"]["Unverified"].is_object());
    }

    #[test]
    fn macro_save_and_assign_continues_through_core_commands() {
        let mut browser = BrowserSession::new().unwrap();
        assert_eq!(load_keymap(&mut browser)["outcome"]["kind"], "loaded");
        let slot = browser.session.macros().unwrap().slot().to_owned();
        let backend_id = browser.session.descriptor().backend_id.clone();
        let read = send(&mut browser, json!({"type": "read", "feature": "macro"}));
        let command = &read["command"];
        let snapshot = macros::Snapshot {
            backend_id,
            slot: slot.clone(),
            revision: vec![1],
            content: macros::Content::Editable(macros::Program {
                repeat_count: 1,
                events: vec![],
            }),
        };
        let loaded = json!({
            "generation": command["generation"], "operation": command["operation"],
            "payload": {"Macro": {"slot": slot, "result": {"Read": {"Ok": snapshot}}}}
        });
        let loaded: Value = serde_json::from_str(&browser.accept(&loaded.to_string())).unwrap();
        assert_eq!(loaded["outcome"]["kind"], "macroLoaded");
        let exported = send(
            &mut browser,
            json!({"type": "exportMacroDocument", "name": "Browser test", "binding": "counted"}),
        );
        assert_eq!(exported["ok"], true);
        let imported = send(
            &mut browser,
            json!({"type": "importMacroDocument", "document": exported["outcome"]["document"]}),
        );
        assert_eq!(imported["outcome"]["name"], "Browser test");
        let edit = send(
            &mut browser,
            json!({
                "type": "edit", "feature": "macro",
                "change": {"Insert": {"at": 0, "event": {"action": {"Key": {"usage": 4, "pressed": true}}, "delay_ms": 5}}}
            }),
        );
        assert_eq!(edit["ok"], true);
        let descriptor = browser.session.descriptor();
        let (layer, key) = descriptor
            .layers
            .iter()
            .find_map(|layer| {
                descriptor
                    .keys
                    .iter()
                    .find(|key| descriptor.key_is_writable(&layer.id, &key.id))
                    .map(|key| (layer.id.clone(), key.id.clone()))
            })
            .unwrap();
        let assign = send(
            &mut browser,
            json!({
                "type": "saveAndAssignMacro", "layer": layer, "key": key, "binding": "counted"
            }),
        );
        assert_eq!(assign["ok"], true);
        let macro_command = &assign["command"];
        let mut applied_macro = macro_command["payload"]["Macro"]["Apply"]["expected"].clone();
        applied_macro["content"] =
            json!({"Editable": macro_command["payload"]["Macro"]["Apply"]["desired"]});
        let macro_done = json!({
            "generation": macro_command["generation"], "operation": macro_command["operation"],
            "payload": {"Macro": {"slot": slot, "result": {"Apply": {"Ok": applied_macro}}}}
        });
        let next: Value = serde_json::from_str(&browser.accept(&macro_done.to_string())).unwrap();
        assert_eq!(next["outcome"]["kind"], "continue");
        let keymap_command = &next["command"];
        let mut applied_keymap: State =
            parse(keymap_command["payload"]["Keymap"]["Apply"]["expected"].clone()).unwrap();
        let changes: Vec<keymap::Change> =
            parse(keymap_command["payload"]["Keymap"]["Apply"]["desired"].clone()).unwrap();
        for change in changes {
            applied_keymap
                .bindings
                .get_mut(&change.layer)
                .unwrap()
                .insert(change.key, change.action);
        }
        let keymap_done = json!({
            "generation": keymap_command["generation"], "operation": keymap_command["operation"],
            "payload": {"Keymap": {"Apply": {"Ok": applied_keymap}}}
        });
        let result: Value =
            serde_json::from_str(&browser.accept(&keymap_done.to_string())).unwrap();
        assert_eq!(result["outcome"]["kind"], "assignmentSucceeded");
        assert_eq!(result["view"]["keymap"]["dirty"], false);
        assert_eq!(result["view"]["macros"]["editor"]["dirty"], false);
    }

    #[test]
    fn lighting_edit_uses_shared_semantic_white_and_core_command() {
        let mut browser = BrowserSession::new().unwrap();
        let _ = send(&mut browser, json!({"type": "connect"}));
        let read = send(&mut browser, json!({"type": "read", "feature": "lighting"}));
        let command = &read["command"];
        let mut raw = [0u8; 64];
        raw[..8].copy_from_slice(&[0x87, 1, 0, 4, 7, 250, 255, 250]);
        let snapshot = lighting_adapter::from_bytes(&raw).unwrap();
        let completion = json!({
            "generation": command["generation"], "operation": command["operation"],
            "payload": {"Lighting": {"Read": {"Ok": snapshot}}}
        });
        let loaded: Value = serde_json::from_str(&browser.accept(&completion.to_string())).unwrap();
        assert_eq!(loaded["outcome"]["kind"], "lightingLoaded");
        assert_eq!(
            loaded["view"]["lighting"]["editor"]["draft"]["color"],
            json!({"Rgb": [255, 255, 255]})
        );
        let edit = send(
            &mut browser,
            json!({
                "type": "edit", "feature": "lighting", "change": {"Color": {"Rgb": [1, 2, 3]}}
            }),
        );
        assert_eq!(edit["ok"], true);
        let apply = send(
            &mut browser,
            json!({"type": "apply", "feature": "lighting"}),
        );
        assert_eq!(apply["ok"], true);
        assert_eq!(
            apply["command"]["payload"]["Lighting"]["Apply"]["desired"]["color"],
            json!({"Rgb": [1, 2, 3]})
        );
    }
}
