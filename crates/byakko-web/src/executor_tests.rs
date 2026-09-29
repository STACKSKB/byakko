use super::*;

fn command(payload: CommandPayload) -> Command {
    Command {
        generation: 7,
        operation: 9,
        payload,
    }
}

fn make_operation(payload: CommandPayload) -> BrowserOperation {
    BrowserOperation::new(&serde_json::to_string(&command(payload)).unwrap()).unwrap()
}

fn step(operation: &BrowserOperation) -> Value {
    serde_json::from_str(&operation.step()).unwrap()
}

fn advance(operation: &mut BrowserOperation, value: Value) {
    operation.advance(&value.to_string()).unwrap();
}

struct Device {
    base: Vec<[u8; 4]>,
    function: Vec<[u8; 4]>,
    lighting: [u8; 64],
    picture: Vec<[u8; 3]>,
    macro_bytes: Vec<u8>,
    effects: Vec<String>,
    writes: Vec<Vec<u8>>,
}

impl Device {
    fn new() -> Self {
        let mut lighting = [0; 64];
        lighting[..8].copy_from_slice(&[0x87, 13, 4, 4, 16, 0, 200, 200]);
        Self {
            base: vec![[0; 4]; 128],
            function: vec![[0; 4]; 128],
            lighting,
            picture: vec![[0; 3]; 128],
            macro_bytes: vec![0; 256],
            effects: Vec::new(),
            writes: Vec::new(),
        }
    }

    fn snapshot(&self) -> adapter::Snapshot {
        adapter::Snapshot {
            format_version: 1,
            firmware: 0x0100,
            profile: 0,
            base: self.base.clone(),
            function: self.function.clone(),
        }
    }

    fn reply(&mut self, report: &[u8]) -> Vec<u8> {
        self.effects.push(format!("read:{:02x}", report[0]));
        match report[0] {
            0x80 => {
                let mut reply = [0; 64];
                reply[..3].copy_from_slice(&[0x80, 0, 1]);
                reply.to_vec()
            }
            0x85 => {
                let mut reply = [0; 64];
                reply[0] = 0x85;
                reply.to_vec()
            }
            0x87 => self.lighting.to_vec(),
            0x89 | 0x90 => {
                let matrix = if report[0] == 0x89 {
                    &self.base
                } else {
                    &self.function
                };
                matrix[usize::from(report[2]) * 16..usize::from(report[2] + 1) * 16]
                    .iter()
                    .flat_map(|slot| slot.iter().copied())
                    .collect()
            }
            0x8b => self.macro_bytes[usize::from(report[2]) * 64..usize::from(report[2] + 1) * 64]
                .to_vec(),
            0x8c => {
                let mut bytes = self
                    .picture
                    .iter()
                    .flat_map(|rgb| rgb.iter().copied())
                    .collect::<Vec<_>>();
                bytes.resize(384, 0);
                bytes[usize::from(report[2]) * 64..usize::from(report[2] + 1) * 64].to_vec()
            }
            opcode => panic!("unexpected read opcode {opcode:02x}"),
        }
    }

    fn write(&mut self, report: &[u8]) {
        self.effects.push(format!("write:{:02x}", report[0]));
        self.writes.push(report.to_vec());
        match report[0] {
            0x13 => self.base[usize::from(report[2])].copy_from_slice(&report[8..12]),
            0x15 => self.function[usize::from(report[2])].copy_from_slice(&report[8..12]),
            0x07 => {
                self.lighting[1..8].copy_from_slice(&report[1..8]);
            }
            0x16 => {
                let start = usize::from(report[2]) * 56;
                let length = usize::from(report[3]);
                self.macro_bytes[start..start + length].copy_from_slice(&report[8..8 + length]);
            }
            0x0c => {
                let mut bytes = self
                    .picture
                    .iter()
                    .flat_map(|rgb| rgb.iter().copied())
                    .collect::<Vec<_>>();
                let start = usize::from(report[4]) * 56;
                let length = (384 - start).min(56);
                bytes[start..start + length].copy_from_slice(&report[8..8 + length]);
                self.picture = bytes.as_chunks::<3>().0.to_vec();
            }
            opcode => panic!("unexpected write opcode {opcode:02x}"),
        }
    }
}

fn run(
    operation: &mut BrowserOperation,
    device: &mut Device,
    fail_write: Option<usize>,
) -> Completion {
    let mut writes = 0;
    for _ in 0..200 {
        let action = step(operation);
        match action["kind"].as_str().unwrap() {
            "backup" => {
                device.effects.push("backup".into());
                advance(operation, json!({"Ok":null}));
            }
            "exchange" => {
                assert_eq!(action["delay_ms"], 30);
                let report: Vec<u8> = serde_json::from_value(action["report"].clone()).unwrap();
                advance(operation, json!({"Ok":device.reply(&report)}));
            }
            "write" => {
                let report: Vec<u8> = serde_json::from_value(action["report"].clone()).unwrap();
                device.write(&report);
                writes += 1;
                if fail_write == Some(writes) {
                    advance(operation, json!({"Err":"injected send failure"}));
                } else {
                    advance(operation, json!({"Ok":null}));
                }
            }
            "complete" => return serde_json::from_value(action["completion"].clone()).unwrap(),
            kind => panic!("unexpected step {kind}"),
        }
    }
    panic!("operation did not terminate");
}

#[test]
fn keymap_apply_backs_up_before_single_key_setter_and_reads_once_afterward() {
    let mut device = Device::new();
    let state = adapter::from_snapshot(&device.snapshot()).unwrap();
    let changes = vec![keymap::Change {
        layer: "base".into(),
        key: adapter::key_id(9),
        action: keymap::Action::Key(4),
    }];
    let mut operation = make_operation(CommandPayload::Keymap(FeatureCommand::Apply {
        expected: state,
        desired: changes,
    }));
    let complete = run(&mut operation, &mut device, None);
    assert_eq!((complete.generation, complete.operation), (7, 9));
    assert!(matches!(
        complete.payload,
        CompletionPayload::Keymap(FeatureResult::Apply(Ok(_)))
    ));
    assert_eq!(device.effects[0], "backup");
    assert_eq!(device.writes.len(), 1);
    assert_eq!((device.writes[0][0], device.writes[0][2]), (0x13, 9));
    assert_eq!(device.base[9], [0, 0, 4, 0]);
    assert_eq!(
        device
            .effects
            .iter()
            .filter(|effect| effect.starts_with("read:"))
            .count(),
        18
    );
}

#[test]
fn failed_keymap_setter_restores_both_maps_and_reports_verified_recovery() {
    let mut device = Device::new();
    let state = adapter::from_snapshot(&device.snapshot()).unwrap();
    let changes = vec![
        keymap::Change {
            layer: "base".into(),
            key: adapter::key_id(9),
            action: keymap::Action::Key(4),
        },
        keymap::Change {
            layer: "fn".into(),
            key: adapter::key_id(10),
            action: keymap::Action::Key(5),
        },
    ];
    let mut operation = make_operation(CommandPayload::Keymap(FeatureCommand::Apply {
        expected: state,
        desired: changes,
    }));
    let complete = run(&mut operation, &mut device, Some(2));
    let CompletionPayload::Keymap(FeatureResult::Apply(Err(failure))) = complete.payload else {
        panic!("wrong result");
    };
    assert_eq!(failure.recovery, Recovery::Verified);
    assert_eq!(device.base[9], [0; 4]);
    assert_eq!(device.function[10], [0; 4]);
    assert_eq!(
        device
            .writes
            .iter()
            .map(|report| report[0])
            .collect::<Vec<_>>(),
        [0x13, 0x15, 0x15, 0x13]
    );
}

#[test]
fn failed_first_open_never_attempts_a_setter_or_recovery() {
    let device = Device::new();
    let state = adapter::from_snapshot(&device.snapshot()).unwrap();
    let changes = vec![keymap::Change {
        layer: "base".into(),
        key: adapter::key_id(9),
        action: keymap::Action::Key(4),
    }];
    let mut operation = make_operation(CommandPayload::Keymap(FeatureCommand::Apply {
        expected: state,
        desired: changes,
    }));
    assert_eq!(step(&operation)["kind"], "backup");
    advance(&mut operation, json!({"Ok":null}));
    assert_eq!(step(&operation)["kind"], "write");
    advance(
        &mut operation,
        json!({"Err":{"message":"open denied","setterAttempted":false}}),
    );
    let result = step(&operation);
    assert_eq!(result["kind"], "complete");
    let complete: Completion = serde_json::from_value(result["completion"].clone()).unwrap();
    let CompletionPayload::Keymap(FeatureResult::Apply(Err(failure))) = complete.payload else {
        panic!("wrong result");
    };
    assert_eq!(failure.recovery, Recovery::NotAttempted);
}

#[test]
fn failed_later_open_recovers_after_an_earlier_setter() {
    let mut device = Device::new();
    let state = adapter::from_snapshot(&device.snapshot()).unwrap();
    let changes = vec![
        keymap::Change {
            layer: "base".into(),
            key: adapter::key_id(9),
            action: keymap::Action::Key(4),
        },
        keymap::Change {
            layer: "base".into(),
            key: adapter::key_id(10),
            action: keymap::Action::Key(5),
        },
    ];
    let mut operation = make_operation(CommandPayload::Keymap(FeatureCommand::Apply {
        expected: state,
        desired: changes,
    }));
    advance(&mut operation, json!({"Ok":null})); // backup
    let first: Vec<u8> = serde_json::from_value(step(&operation)["report"].clone()).unwrap();
    device.write(&first);
    advance(&mut operation, json!({"Ok":null}));
    assert_eq!(step(&operation)["kind"], "write");
    advance(
        &mut operation,
        json!({"Err":{"message":"second open failed","setterAttempted":false}}),
    );
    let complete = run(&mut operation, &mut device, None);
    let CompletionPayload::Keymap(FeatureResult::Apply(Err(failure))) = complete.payload else {
        panic!("wrong result");
    };
    assert_eq!(failure.recovery, Recovery::Verified);
    assert_eq!(device.base[9], [0; 4]);
    assert_eq!(device.base[10], [0; 4]);
}

#[test]
fn lighting_and_picture_accept_transport_without_getters() {
    let mut device = Device::new();
    let before = lighting_adapter::from_bytes(&device.lighting).unwrap();
    let lighting::Content::Editable(mut desired) = before.content.clone() else {
        panic!("lighting fixture");
    };
    desired.brightness = Some(3);
    let mut operation = make_operation(CommandPayload::Lighting(FeatureCommand::Apply {
        expected: before,
        desired,
    }));
    let complete = run(&mut operation, &mut device, None);
    let CompletionPayload::Lighting(FeatureResult::Apply(Ok(saved))) = complete.payload else {
        panic!("wrong lighting result");
    };
    assert_eq!(saved.evidence, lighting::Evidence::TransportAccepted);
    assert_eq!(device.effects, ["backup", "write:07"]);

    device.effects.clear();
    let before = picture_adapter::project(&device.picture, [13, 1]).unwrap();
    let picture::Content::Editable(mut desired) = before.content.clone() else {
        panic!("picture fixture");
    };
    *desired.values_mut().next().unwrap() = [12, 34, 56];
    let mut operation = make_operation(CommandPayload::Picture(FeatureCommand::Apply {
        expected: before,
        desired,
    }));
    let complete = run(&mut operation, &mut device, None);
    let CompletionPayload::Picture(FeatureResult::Apply(Ok(saved))) = complete.payload else {
        panic!("wrong picture result");
    };
    assert_eq!(saved.evidence, picture::Evidence::TransportAccepted);
    assert_eq!(
        saved.revision,
        device
            .picture
            .iter()
            .flat_map(|rgb| rgb.iter().copied())
            .collect::<Vec<_>>()
    );
    assert_eq!(device.effects[0], "backup");
    assert_eq!(device.effects.len(), 8);
    assert!(
        device.effects[1..]
            .iter()
            .all(|effect| effect == "write:0c")
    );
}

#[test]
fn macro_five_page_write_preserves_short_final_page_and_reads_once() {
    let mut device = Device::new();
    let expected = macro_adapter::from_bytes("slot-49", &device.macro_bytes).unwrap();
    let desired = macros::Program {
        repeat_count: 1,
        events: Vec::new(),
    };
    let mut operation = make_operation(CommandPayload::Macro(FeatureCommand::Apply {
        expected,
        desired,
    }));
    let complete = run(&mut operation, &mut device, None);
    assert!(matches!(
        complete.payload,
        CompletionPayload::Macro {
            result: FeatureResult::Apply(Ok(_)),
            ..
        }
    ));
    assert_eq!(device.effects[0], "backup");
    assert_eq!(device.writes.len(), 5);
    assert_eq!(device.writes[4][3], 26);
    assert_eq!(
        device
            .effects
            .iter()
            .filter(|effect| effect == &&"read:8b".to_owned())
            .count(),
        4
    );
}

#[test]
fn failed_macro_page_restores_exact_cached_bytes() {
    let mut device = Device::new();
    let original = device.macro_bytes.clone();
    let expected = macro_adapter::from_bytes("slot-49", &original).unwrap();
    let desired = macros::Program {
        repeat_count: 1,
        events: Vec::new(),
    };
    let mut operation = make_operation(CommandPayload::Macro(FeatureCommand::Apply {
        expected,
        desired,
    }));
    let complete = run(&mut operation, &mut device, Some(2));
    let CompletionPayload::Macro {
        result: FeatureResult::Apply(Err(failure)),
        ..
    } = complete.payload
    else {
        panic!("wrong macro result");
    };
    assert_eq!(failure.recovery, Recovery::Verified);
    assert_eq!(device.macro_bytes, original);
    assert_eq!(device.writes.len(), 7);
    assert_eq!(device.writes.last().unwrap()[3], 26);
}

#[test]
fn catalog_can_yield_only_after_a_whole_slot() {
    let mut device = Device::new();
    let mut operation = make_operation(CommandPayload::ReadMacroCatalog {
        slots: vec!["slot-00".into(), "slot-01".into()],
    });
    assert!(!operation.can_yield());
    for page in 0..4 {
        let action = step(&operation);
        assert_eq!(action["kind"], "exchange");
        let report: Vec<u8> = serde_json::from_value(action["report"].clone()).unwrap();
        assert_eq!((report[0], report[1], report[2]), (0x8b, 0, page));
        advance(&mut operation, json!({"Ok":device.reply(&report)}));
        assert_eq!(operation.can_yield(), page == 3);
    }
    let action = step(&operation);
    let report: Vec<u8> = serde_json::from_value(action["report"].clone()).unwrap();
    assert_eq!((report[0], report[1]), (0x8b, 1));
    advance(&mut operation, json!({"Ok":device.reply(&report)}));
    assert!(!operation.can_yield());
}
