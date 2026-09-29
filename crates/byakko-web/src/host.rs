//! Pure host-lighting transaction planner. JavaScript owns WebHID effects and
//! durable backup storage; this module owns bytes, order, and verification.
use byakko_core::{
    contract::{
        ApplyFailure, HostEvent, HostEventKind, HostStart, HostTicket, HostUpdate, Recovery,
    },
    editor::{Feature, lighting::LightingRules},
    model::lighting::{HostFrame, HostMode, Snapshot},
};
use byakko_protocol::nia87::{host_adapter, host_lighting, lighting as native, lighting_adapter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserHostActivity {
    ticket: HostTicket,
    mode: HostMode,
    expected: Snapshot,
    saved: native::Lighting,
    active: native::Lighting,
}

#[derive(Clone)]
enum Step {
    Backup(Value),
    Write { report: [u8; 64], after_ms: u32 },
    Exchange([u8; 64]),
    Complete(Value),
}

enum Plan {
    Start {
        activity: BrowserHostActivity,
        target: [u8; 64],
        stage: StartStage,
        cause: Option<String>,
    },
    Stop {
        activity: BrowserHostActivity,
        target: [u8; 64],
        problem: Option<String>,
    },
    Update {
        activity: BrowserHostActivity,
        target: [u8; 64],
    },
    Frame(BrowserHostActivity),
    Terminal,
}

#[derive(Clone, Copy)]
enum StartStage {
    Backup,
    Write,
    Verify,
    RestoreWrite,
    RestoreVerify,
}

#[wasm_bindgen]
pub struct BrowserHostOperation {
    plan: Plan,
    step: Step,
    pending: std::collections::VecDeque<Step>,
}

#[wasm_bindgen]
impl BrowserHostOperation {
    /// `start_json` is the core `HostStart` returned by `Session::start_host`.
    pub fn start(start_json: &str) -> Result<BrowserHostOperation, JsValue> {
        let start: HostStart = parse(start_json)?;
        let original = match host_adapter::baseline(&start.expected).and_then(|saved| {
            host_adapter::native_mode(&start.mode, start.setting.as_ref())
                .map(|setting| (saved, setting))
        }) {
            Ok(original) => original,
            Err(failure) => return Ok(Self::finished_failure(start.ticket, failure)),
        };
        let (saved, setting) = original;
        let target = match native::write_report(&setting) {
            Ok(report) => report,
            Err(error) => {
                return Ok(Self::finished_failure(
                    start.ticket,
                    failure(error, Recovery::NotAttempted),
                ));
            }
        };
        let activity = BrowserHostActivity {
            ticket: start.ticket,
            mode: start.mode,
            expected: start.expected,
            saved: saved.clone(),
            active: saved.clone(),
        };
        if host_adapter::lighting_matches_report(&saved, &target, &saved) {
            return Ok(Self::complete(
                json!({"event": HostEvent {ticket: activity.ticket, kind: HostEventKind::Started}, "activity": activity} ),
            ));
        }
        let backup = json!({
            "format_version": 1,
            "feature": "host-lighting",
            "generation": activity.ticket.generation,
            "operation": activity.ticket.operation,
            "before": saved,
            "target_report": target.as_slice(),
        });
        Ok(Self {
            plan: Plan::Start {
                activity,
                target,
                stage: StartStage::Backup,
                cause: None,
            },
            step: Step::Backup(backup),
            pending: Default::default(),
        })
    }

    /// A failed stop never writes the active host setting as a recovery target.
    pub fn stop(
        ticket_json: &str,
        activity_json: &str,
        problem: Option<String>,
    ) -> Result<BrowserHostOperation, JsValue> {
        let ticket: HostTicket = parse(ticket_json)?;
        let activity: BrowserHostActivity = parse(activity_json)?;
        if ticket != activity.ticket {
            return Err(js("Host ticket differs from active activity"));
        }
        let setting = activity
            .saved
            .recognized_setting()
            .ok_or_else(|| js("Unrecognized saved lighting"))?;
        let target = native::write_report(&setting).map_err(|e| js(&e))?;
        if host_adapter::lighting_matches_report(&activity.active, &target, &activity.active) {
            return Ok(Self::finished_restored(activity, problem));
        }
        let backup = json!({
            "format_version": 1,
            "feature": "host-lighting-restore",
            "generation": ticket.generation,
            "operation": ticket.operation,
            "before": activity.active,
            "target_report": target.as_slice(),
        });
        let mut pending = std::collections::VecDeque::new();
        pending.push_back(Step::Write {
            report: target,
            after_ms: 500,
        });
        pending.push_back(Step::Exchange(native::read_request()));
        Ok(Self {
            plan: Plan::Stop {
                activity,
                target,
                problem,
            },
            step: Step::Backup(backup),
            pending,
        })
    }

    /// Parameter changes retain the immutable original baseline. As on native,
    /// their ordinary setter has pacing but no automatic getter.
    pub fn update(update_json: &str, activity_json: &str) -> Result<BrowserHostOperation, JsValue> {
        let update: HostUpdate = parse(update_json)?;
        let activity: BrowserHostActivity = parse(activity_json)?;
        if update.ticket != activity.ticket {
            return Err(js("Host ticket differs from active activity"));
        }
        let setting = host_adapter::native_mode(&activity.mode, Some(&update.setting))
            .map_err(|e| js(&e.message))?;
        if setting.effect_id != activity.active.effect_id() || !matches!(setting.effect_id, 20 | 22)
        {
            return Err(js("Parameter update must retain the active music mode"));
        }
        let target = native::write_report(&setting).map_err(|e| js(&e))?;
        Ok(Self {
            plan: Plan::Update { activity, target },
            step: Step::Write {
                report: target,
                after_ms: 500,
            },
            pending: Default::default(),
        })
    }

    pub fn frame(frame_json: &str, activity_json: &str) -> Result<BrowserHostOperation, JsValue> {
        let frame: HostFrame = parse(frame_json)?;
        let activity: BrowserHostActivity = parse(activity_json)?;
        frame
            .validate_for(activity.mode.source)
            .map_err(|e| js(&e))?;
        let report = match frame {
            HostFrame::Rgb(rgb) if activity.active.effect_id() == 21 => {
                host_lighting::screen_report(rgb)
            }
            HostFrame::Bands(bands) if matches!(activity.active.effect_id(), 20 | 22) => {
                host_lighting::music_report(
                    bands
                        .try_into()
                        .map_err(|_| js("Nia87 requires 32 audio bands"))?,
                )
            }
            _ => return Err(js("Host frame differs from active mode")),
        };
        Ok(Self {
            plan: Plan::Frame(activity),
            step: Step::Write {
                report,
                after_ms: 0,
            },
            pending: Default::default(),
        })
    }

    pub fn step(&self) -> String {
        self.step_value().to_string()
    }

    pub fn advance(&mut self, response_json: &str) -> Result<String, JsValue> {
        if matches!(self.step, Step::Complete(_)) {
            return Ok(self.step());
        }
        let response: Result<Option<Vec<u8>>, Value> = parse(response_json)?;
        let current = self.step.clone();
        if let Err(error) = response.as_ref() {
            let attempted = error
                .get("setterAttempted")
                .and_then(Value::as_bool)
                .unwrap_or(!matches!(current, Step::Backup(_)));
            self.effect_error(effect_message(error), attempted);
            return Ok(self.step());
        }
        let value = response.unwrap();
        match current {
            Step::Backup(_) | Step::Write { .. } if value.is_some() => {
                self.effect_error(
                    "Effect acknowledgement must be null".into(),
                    matches!(current, Step::Write { .. }),
                );
                return Ok(self.step());
            }
            Step::Exchange(_) => {
                let result = value
                    .ok_or_else(|| "Lighting exchange requires a 64-byte reply".to_string())
                    .and_then(|bytes| {
                        if bytes.first() != Some(&native::LED_READ_COMMAND) {
                            return Err("Lighting read returned an unrelated opcode".into());
                        }
                        native::Lighting::decode(&bytes)
                    });
                self.exchange_done(result);
                return Ok(self.step());
            }
            _ => {}
        }
        self.effect_done(current);
        Ok(self.step())
    }
}

impl BrowserHostOperation {
    fn step_value(&self) -> Value {
        match &self.step {
            Step::Backup(record) => json!({"kind":"backup","record":record}),
            Step::Write { report, after_ms } => {
                json!({"kind":"write","report":report.as_slice(),"before_ms":0,"after_ms":after_ms})
            }
            Step::Exchange(report) => {
                json!({"kind":"exchange","report":report.as_slice(),"delay_ms":30})
            }
            Step::Complete(result) => json!({"kind":"complete","result":result}),
        }
    }
    fn complete(result: Value) -> Self {
        Self {
            plan: Plan::Terminal,
            step: Step::Complete(result),
            pending: Default::default(),
        }
    }
    fn finished_failure(ticket: HostTicket, failure: ApplyFailure) -> Self {
        Self::complete(
            json!({"event":HostEvent{ticket,kind:HostEventKind::Finished{restored:Err(failure),problem:None}},"activity":null}),
        )
    }
    fn finished_restored(activity: BrowserHostActivity, problem: Option<String>) -> Self {
        Self::complete(
            json!({"event":HostEvent{ticket:activity.ticket,kind:HostEventKind::Finished{restored:Ok(activity.expected),problem}},"activity":null}),
        )
    }
    fn effect_done(&mut self, current: Step) {
        match &mut self.plan {
            Plan::Start { stage, target, .. } => match current {
                Step::Backup(_) => {
                    *stage = StartStage::Write;
                    self.step = Step::Write {
                        report: *target,
                        after_ms: 500,
                    };
                }
                Step::Write { .. } if matches!(stage, StartStage::Write) => {
                    *stage = StartStage::Verify;
                    self.step = Step::Exchange(native::read_request());
                }
                Step::Write { .. } if matches!(stage, StartStage::RestoreWrite) => {
                    *stage = StartStage::RestoreVerify;
                    self.step = Step::Exchange(native::read_request());
                }
                _ => unreachable!(),
            },
            Plan::Stop { .. } => {
                self.step = self.pending.pop_front().expect("stop has ordered effects");
            }
            Plan::Update { activity, target } => {
                let result = host_adapter::submitted_lighting(&activity.active, target);
                match result {
                    Ok(active) => {
                        activity.active = active;
                        self.step = Step::Complete(json!({"activity":activity,"error":null}));
                        self.plan = Plan::Terminal;
                    }
                    Err(error) => self.effect_error(error, true),
                }
            }
            Plan::Frame(activity) => {
                self.step = Step::Complete(json!({"activity":activity,"error":null}));
                self.plan = Plan::Terminal;
            }
            Plan::Terminal => unreachable!(),
        }
    }
    fn exchange_done(&mut self, result: Result<native::Lighting, String>) {
        match &self.plan {
            Plan::Start {
                activity,
                target,
                stage: StartStage::Verify,
                ..
            } => {
                let success = result.as_ref().is_ok_and(|raw| {
                    host_adapter::lighting_matches_report(raw, target, &activity.saved)
                });
                if success {
                    let mut activity = activity.clone();
                    activity.active = result.unwrap();
                    self.step = Step::Complete(
                        json!({"event":HostEvent{ticket:activity.ticket,kind:HostEventKind::Started},"activity":activity}),
                    );
                    self.plan = Plan::Terminal;
                } else {
                    self.begin_start_restore(result.err().unwrap_or_else(|| {
                        "Lighting readback differs in setting or reserved response bytes".into()
                    }));
                }
            }
            Plan::Start {
                activity,
                stage: StartStage::RestoreVerify,
                cause,
                ..
            } => {
                let verified = result.as_ref().is_ok_and(|raw| {
                    host_adapter::lighting_matches_report(
                        raw,
                        &host_adapter::lighting_restore_report(&activity.saved),
                        &activity.saved,
                    )
                });
                let recovery = if verified {
                    Recovery::Verified
                } else if result.is_ok() {
                    Recovery::Failed
                } else {
                    Recovery::Unverified
                };
                let message = format!(
                    "Host lighting start failed: {}; restoration {}",
                    cause.as_deref().unwrap_or("unknown error"),
                    if verified { "verified" } else { "not verified" }
                );
                let ticket = activity.ticket;
                self.step = Step::Complete(
                    json!({"event":HostEvent{ticket,kind:HostEventKind::Finished{restored:Err(failure(message,recovery)),problem:None}},"activity":null}),
                );
                self.plan = Plan::Terminal;
            }
            Plan::Stop {
                activity,
                target,
                problem,
            } => {
                let verified = result.as_ref().is_ok_and(|raw| {
                    host_adapter::lighting_matches_report(raw, target, &activity.active)
                });
                let result = if verified {
                    let snapshot = lighting_adapter::from_native(&result.unwrap());
                    if LightingRules::same_baseline(&snapshot, &activity.expected) {
                        Ok(snapshot)
                    } else {
                        Err(failure(
                            "Host lighting readback differs from saved baseline".into(),
                            Recovery::Unverified,
                        ))
                    }
                } else {
                    Err(failure(
                        result.err().unwrap_or_else(|| {
                            "Host lighting restoration readback mismatch".into()
                        }),
                        Recovery::Unverified,
                    ))
                };
                self.step = Step::Complete(
                    json!({"event":HostEvent{ticket:activity.ticket,kind:HostEventKind::Finished{restored:result,problem:problem.clone()}},"activity":null}),
                );
                self.plan = Plan::Terminal;
            }
            _ => unreachable!(),
        }
    }
    fn begin_start_restore(&mut self, cause: String) {
        if let Plan::Start {
            activity,
            stage,
            cause: original,
            ..
        } = &mut self.plan
        {
            *stage = StartStage::RestoreWrite;
            *original = Some(cause);
            self.step = Step::Write {
                report: host_adapter::lighting_restore_report(&activity.saved),
                after_ms: 500,
            };
        }
    }
    fn effect_error(&mut self, message: String, attempted: bool) {
        match &self.plan {
            Plan::Start {
                activity, stage, ..
            } => {
                if matches!(stage, StartStage::Backup)
                    || (matches!(stage, StartStage::Write) && !attempted)
                {
                    let ticket = activity.ticket;
                    self.step = Step::Complete(
                        json!({"event":HostEvent{ticket,kind:HostEventKind::Finished{restored:Err(failure(message,Recovery::NotAttempted)),problem:None}},"activity":null}),
                    );
                    self.plan = Plan::Terminal;
                } else if matches!(stage, StartStage::RestoreWrite | StartStage::RestoreVerify) {
                    let ticket = activity.ticket;
                    self.step = Step::Complete(
                        json!({"event":HostEvent{ticket,kind:HostEventKind::Finished{restored:Err(failure(format!("Host lighting restoration unverified: {message}"),Recovery::Unverified)),problem:None}},"activity":null}),
                    );
                    self.plan = Plan::Terminal;
                } else {
                    self.begin_start_restore(message);
                }
            }
            Plan::Stop {
                activity, problem, ..
            } => {
                let ticket = activity.ticket;
                self.step = Step::Complete(
                    json!({"event":HostEvent{ticket,kind:HostEventKind::Finished{restored:Err(failure(message,Recovery::Unverified)),problem:problem.clone()}},"activity":null}),
                );
                self.plan = Plan::Terminal;
            }
            Plan::Update { activity, .. } | Plan::Frame(activity) => {
                self.step = Step::Complete(json!({"activity":activity,"error":message}));
                self.plan = Plan::Terminal;
            }
            Plan::Terminal => {}
        }
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, JsValue> {
    serde_json::from_str(value).map_err(|e| js(&e.to_string()))
}
fn js(message: &str) -> JsValue {
    JsValue::from_str(message)
}
fn failure(message: String, recovery: Recovery) -> ApplyFailure {
    ApplyFailure { message, recovery }
}
fn effect_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Host lighting effect failed")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::lighting::Content;

    fn original() -> native::Lighting {
        let mut raw = [0u8; 64];
        raw[..8].copy_from_slice(&[native::LED_READ_COMMAND, 1, 4, 4, 0, 7, 8, 9]);
        raw[9] = 0xa5;
        native::Lighting::decode(&raw).unwrap()
    }

    fn start(mode_id: &str) -> BrowserHostOperation {
        let mode = lighting_adapter::capabilities()
            .host_modes
            .into_iter()
            .find(|mode| mode.id == mode_id)
            .unwrap();
        let setting = mode
            .parameters
            .as_ref()
            .map(|parameters| parameters.default.clone());
        let start = HostStart {
            ticket: HostTicket {
                generation: 2,
                operation: 9,
            },
            mode,
            setting,
            expected: lighting_adapter::from_native(&original()),
        };
        BrowserHostOperation::start(&serde_json::to_string(&start).unwrap()).unwrap()
    }

    fn next(operation: &mut BrowserHostOperation, answer: Value) -> Value {
        let step = operation.advance(&answer.to_string()).unwrap();
        serde_json::from_str(&step).unwrap()
    }

    fn acknowledged(operation: &mut BrowserHostOperation) -> Value {
        next(operation, json!({"Ok":null}))
    }

    fn reply_for(report: &[u8], opaque: u8) -> Value {
        let mut raw = [0u8; 64];
        raw[0] = native::LED_READ_COMMAND;
        raw[1..8].copy_from_slice(&report[1..8]);
        raw[9] = opaque;
        json!({"Ok":raw.as_slice()})
    }

    fn started(mode: &str) -> BrowserHostActivity {
        let mut operation = start(mode);
        assert_eq!(operation.step_value()["kind"], "backup");
        let write = acknowledged(&mut operation);
        assert_eq!(write["after_ms"], 500);
        let report: Vec<u8> = serde_json::from_value(write["report"].clone()).unwrap();
        assert_eq!(acknowledged(&mut operation)["kind"], "exchange");
        let complete = next(&mut operation, reply_for(&report, 0xa5));
        assert_eq!(complete["result"]["event"]["kind"], "Started");
        serde_json::from_value(complete["result"]["activity"].clone()).unwrap()
    }

    #[test]
    fn start_failure_restores_once_and_verifies_original_bytes() {
        let mut operation = start("screen-average");
        acknowledged(&mut operation);
        let restore = next(
            &mut operation,
            json!({"Err":{"message":"setter failed","setterAttempted":true}}),
        );
        assert_eq!(restore["kind"], "write");
        assert_eq!(restore["after_ms"], 500);
        let report: Vec<u8> = serde_json::from_value(restore["report"].clone()).unwrap();
        assert_eq!(report, host_adapter::lighting_restore_report(&original()));
        assert_eq!(acknowledged(&mut operation)["kind"], "exchange");
        let complete = next(&mut operation, json!({"Ok":original().raw()}));
        assert_eq!(
            complete["result"]["event"]["kind"]["Finished"]["restored"]["Err"]["recovery"],
            "Verified"
        );
        assert_eq!(complete["result"]["activity"], Value::Null);
    }

    #[test]
    fn start_read_failure_recovers_even_when_exchange_did_not_send_a_setter() {
        let mut operation = start("screen-average");
        acknowledged(&mut operation);
        acknowledged(&mut operation);
        let restore = next(
            &mut operation,
            json!({"Err":{"message":"read failed","setterAttempted":false}}),
        );
        assert_eq!(restore["kind"], "write");
        assert_eq!(
            restore["report"],
            json!(host_adapter::lighting_restore_report(&original()).as_slice())
        );
        acknowledged(&mut operation);
        let complete = next(
            &mut operation,
            json!({"Err":{"message":"restore read failed","setterAttempted":false}}),
        );
        assert_eq!(
            complete["result"]["event"]["kind"]["Finished"]["restored"]["Err"]["recovery"],
            "Unverified"
        );
    }

    #[test]
    fn stop_backup_failure_reports_unverified_and_never_writes_active_mode() {
        let activity = started("screen-average");
        let mut operation = BrowserHostOperation::stop(
            &serde_json::to_string(&activity.ticket).unwrap(),
            &serde_json::to_string(&activity).unwrap(),
            None,
        )
        .unwrap();
        let complete = next(
            &mut operation,
            json!({"Err":{"message":"backup failed","setterAttempted":false}}),
        );
        assert_eq!(complete["kind"], "complete");
        assert_eq!(
            complete["result"]["event"]["kind"]["Finished"]["restored"]["Err"]["recovery"],
            "Unverified"
        );
    }

    #[test]
    fn stop_mismatch_never_reinstates_active_host_mode() {
        let activity = started("screen-average");
        let mut operation = BrowserHostOperation::stop(
            &serde_json::to_string(&activity.ticket).unwrap(),
            &serde_json::to_string(&activity).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(operation.step_value()["kind"], "backup");
        let write = acknowledged(&mut operation);
        assert_eq!(write["after_ms"], 500);
        assert_eq!(acknowledged(&mut operation)["kind"], "exchange");
        let mut wrong = original().raw().to_vec();
        wrong[1] = 2;
        let complete = next(&mut operation, json!({"Ok":wrong}));
        assert_eq!(complete["kind"], "complete");
        assert_eq!(
            complete["result"]["event"]["kind"]["Finished"]["restored"]["Err"]["recovery"],
            "Unverified"
        );
    }

    #[test]
    fn parameter_update_keeps_original_baseline_and_frame_encodes_exact_bytes() {
        let activity = started("music-follow-2");
        let saved = activity.saved.clone();
        let mode = &activity.mode;
        let mut setting = mode.parameters.as_ref().unwrap().default.clone();
        setting.brightness = Some(3);
        let update = HostUpdate {
            ticket: activity.ticket,
            setting,
        };
        let mut operation = BrowserHostOperation::update(
            &serde_json::to_string(&update).unwrap(),
            &serde_json::to_string(&activity).unwrap(),
        )
        .unwrap();
        let report: Vec<u8> =
            serde_json::from_value(operation.step_value()["report"].clone()).unwrap();
        assert_eq!(report[1], 22);
        assert_eq!(report[3], 3);
        let result = acknowledged(&mut operation);
        let updated: BrowserHostActivity =
            serde_json::from_value(result["result"]["activity"].clone()).unwrap();
        assert_eq!(updated.saved, saved);
        assert_ne!(updated.active, saved);

        let bands: Vec<u8> = (0..32).collect();
        let frame = HostFrame::Bands(bands.clone());
        let mut operation = BrowserHostOperation::frame(
            &serde_json::to_string(&frame).unwrap(),
            &serde_json::to_string(&updated).unwrap(),
        )
        .unwrap();
        let actual: Vec<u8> =
            serde_json::from_value(operation.step_value()["report"].clone()).unwrap();
        assert_eq!(
            actual,
            host_lighting::music_report(bands.try_into().unwrap())
        );
        assert_eq!(acknowledged(&mut operation)["result"]["error"], Value::Null);
        assert!(matches!(updated.expected.content, Content::Editable(_)));
    }
}
