//! US layout presets resolved from immutable physical positions, never current bindings.
use crate::{
    model::keymap::{Action, Change, Descriptor},
    validation::keymap::{validate_changes, validate_edit},
};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypingPreset {
    Dvorak,
    Colemak,
}

pub const ALL: [TypingPreset; 2] = [TypingPreset::Dvorak, TypingPreset::Colemak];

impl fmt::Display for TypingPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dvorak => "Dvorak",
            Self::Colemak => "Colemak",
        })
    }
}

// Standard USB QWERTY positions: minus/equal, Q..], A..', Z../.
const POSITIONS: [u16; 35] = [
    0x2d, 0x2e, 0x14, 0x1a, 0x08, 0x15, 0x17, 0x1c, 0x18, 0x0c, 0x12, 0x13, 0x2f, 0x30, 0x04, 0x16,
    0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f, 0x33, 0x34, 0x1d, 0x1b, 0x06, 0x19, 0x05, 0x11, 0x10,
    0x36, 0x37, 0x38,
];

impl TypingPreset {
    pub const ALL: [Self; 2] = ALL;

    fn usages(self) -> [u16; 35] {
        // Base symbols from xkbcommon's US dvorak and colemak definitions:
        // https://github.com/xkbcommon/libxkbcommon/blob/master/test/data/symbols/us
        // Colemak is Shai Coleman's layout: https://colemak.com/ . Caps Lock is untouched.
        match self {
            Self::Dvorak => [
                0x2f, 0x30, 0x34, 0x36, 0x37, 0x13, 0x1c, 0x09, 0x0a, 0x06, 0x15, 0x0f, 0x38, 0x2e,
                0x04, 0x12, 0x08, 0x18, 0x0c, 0x07, 0x0b, 0x17, 0x11, 0x16, 0x2d, 0x33, 0x14, 0x0d,
                0x0e, 0x1b, 0x05, 0x10, 0x1a, 0x19, 0x1d,
            ],
            Self::Colemak => [
                0x2d, 0x2e, 0x14, 0x1a, 0x09, 0x13, 0x0a, 0x0d, 0x0f, 0x18, 0x1c, 0x33, 0x2f, 0x30,
                0x04, 0x15, 0x16, 0x17, 0x07, 0x0b, 0x11, 0x08, 0x0c, 0x12, 0x34, 0x1d, 0x1b, 0x06,
                0x19, 0x05, 0x0e, 0x10, 0x36, 0x37, 0x38,
            ],
        }
    }
}

/// Plan every preset position, including unchanged symbols so switching is deterministic.
/// Other positions and layers are deliberately absent from the result.
pub fn plan(
    descriptor: &Descriptor,
    layer: &str,
    preset: TypingPreset,
) -> Result<Vec<Change>, String> {
    let mut changes = Vec::with_capacity(POSITIONS.len());
    for (position, usage) in POSITIONS.into_iter().zip(preset.usages()) {
        let mut keys = descriptor
            .keys
            .iter()
            .filter(|key| key.position == Some(position));
        let key = keys
            .next()
            .ok_or_else(|| format!("Missing physical position {position:#04x}"))?;
        if keys.next().is_some() {
            return Err(format!("Duplicate physical position {position:#04x}"));
        }
        changes.push(Change {
            layer: layer.into(),
            key: key.id.clone(),
            action: Action::Key(usage),
        });
    }
    validate_changes(descriptor, &changes)?;
    for change in &changes {
        validate_edit(descriptor, change)?;
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract::{ApplyFailure, Problem, Recovery},
        contract::{Command, Completion, CompletionPayload, FeatureResult},
        editor::{Editor, Status, keymap::KeymapRules},
        model::keymap::{ActionCategory, ActionChoice, Layer, PhysicalKey, State},
        session::{Outcome, Session},
    };

    fn descriptor() -> Descriptor {
        Descriptor {
            backend_id: "nia87".into(),
            device_name: "Test".into(),
            keys: POSITIONS
                .into_iter()
                .chain([0x39, 0xe1, 0x28])
                .map(|position| PhysicalKey {
                    id: format!("slot-{position}"),
                    label: "not a position".into(),
                    position: Some(position),
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                    visible: true,
                    writable: true,
                })
                .collect(),
            layers: ["base", "fn"]
                .map(|id| Layer {
                    id: id.into(),
                    label: id.into(),
                    read_only_keys: vec![],
                })
                .into(),
            actions: (4..=0xe7)
                .map(|usage| ActionChoice {
                    label: usage.to_string(),
                    action: Action::Key(usage),
                    category: ActionCategory::Other,
                })
                .collect(),
            shortcuts: None,
        }
    }

    fn state(d: &Descriptor) -> State {
        State {
            revision: vec![1],
            bindings: d
                .layers
                .iter()
                .map(|layer| {
                    (
                        layer.id.clone(),
                        d.keys
                            .iter()
                            .map(|key|
                // Deliberately unrelated to physical position and label.
                (key.id.clone(), Action::Key(0x29)))
                            .collect(),
                    )
                })
                .collect(),
        }
    }

    fn editor(d: Descriptor) -> Editor<KeymapRules> {
        let baseline = state(&d);
        let mut editor = Editor::new(KeymapRules::new(d).unwrap());
        editor.accept_read(Ok(baseline));
        editor
    }

    fn ascii_usage(symbol: char) -> u16 {
        match symbol {
            'a'..='z' => symbol as u16 - 'a' as u16 + 4,
            '-' => 0x2d,
            '=' => 0x2e,
            '[' => 0x2f,
            ']' => 0x30,
            ';' => 0x33,
            '\'' => 0x34,
            ',' => 0x36,
            '.' => 0x37,
            '/' => 0x38,
            _ => panic!("Unexpected symbol"),
        }
    }

    #[test]
    fn complete_vectors_follow_physical_qwerty_positions() {
        for (preset, symbols) in [
            (TypingPreset::Dvorak, "[]',.pyfgcrl/=aoeuidhtns-;qjkxbmwvz"),
            (TypingPreset::Colemak, "-=qwfpgjluy;[]arstdhneio'zxcvbkm,./"),
        ] {
            let mut d = descriptor();
            d.keys.reverse();
            let changes = plan(&d, "base", preset).unwrap();
            assert_eq!(changes.len(), 35);
            assert_eq!(
                changes.iter().map(|c| c.action.clone()).collect::<Vec<_>>(),
                symbols
                    .chars()
                    .map(|c| Action::Key(ascii_usage(c)))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                changes.iter().map(|c| c.key.clone()).collect::<Vec<_>>(),
                POSITIONS.map(|p| format!("slot-{p}")).to_vec()
            );
        }
        assert_eq!(ALL.map(|preset| preset.to_string()), ["Dvorak", "Colemak"]);
    }

    #[test]
    fn missing_duplicate_unsupported_and_protected_positions_are_rejected() {
        let d = descriptor();
        let mut missing = d.clone();
        missing.keys[0].position = None;
        assert!(plan(&missing, "base", TypingPreset::Dvorak).is_err());
        let mut duplicate = d.clone();
        let mut key = duplicate.keys[0].clone();
        key.id = "duplicate".into();
        duplicate.keys.push(key);
        assert!(plan(&duplicate, "base", TypingPreset::Dvorak).is_err());
        let mut unsupported = d.clone();
        unsupported
            .actions
            .retain(|choice| choice.action != Action::Key(0x2f));
        assert!(plan(&unsupported, "base", TypingPreset::Dvorak).is_err());
        let mut protected = d.clone();
        protected.layers[1]
            .read_only_keys
            .push(protected.keys[34].id.clone());
        assert!(plan(&protected, "fn", TypingPreset::Colemak).is_err());
        protected.keys[34].writable = false;
        assert!(plan(&protected, "base", TypingPreset::Colemak).is_err());
        assert!(plan(&d, "unknown", TypingPreset::Dvorak).is_err());
    }

    #[test]
    fn batch_rejections_are_atomic() {
        let mut d = descriptor();
        d.layers[1].read_only_keys.push(d.keys[34].id.clone());
        let mut e = editor(d.clone());
        let before = e.draft().cloned();
        let changes = plan(&d, "base", TypingPreset::Dvorak).unwrap();
        let mut protected = changes.clone();
        for change in &mut protected {
            change.layer = "fn".into();
        }
        assert!(e.edit_batch(protected).is_err());
        assert_eq!(e.draft(), before.as_ref());
        let mut duplicate = changes.clone();
        duplicate.push(changes[0].clone());
        assert!(e.edit_batch(duplicate).is_err());
        assert_eq!(e.draft(), before.as_ref());
        let mut unsupported = changes;
        unsupported.last_mut().unwrap().action = Action::Key(0xffff);
        assert!(e.edit_batch(unsupported).is_err());
        assert_eq!(e.draft(), before.as_ref());
        let mut unloaded = Editor::new(KeymapRules::new(d).unwrap());
        assert!(unloaded.edit_batch(vec![]).is_err());
    }

    #[test]
    fn repeat_switch_and_revert_preserve_unspecified_positions_and_other_layers() {
        let d = descriptor();
        let mut e = editor(d.clone());
        let original = e.draft().unwrap().clone();
        e.edit_batch(plan(&d, "base", TypingPreset::Dvorak).unwrap())
            .unwrap();
        let dvorak = e.draft().cloned();
        e.edit_batch(plan(&d, "base", TypingPreset::Dvorak).unwrap())
            .unwrap();
        assert_eq!(e.draft(), dvorak.as_ref());
        e.edit_batch(plan(&d, "base", TypingPreset::Colemak).unwrap())
            .unwrap();
        for change in plan(&d, "base", TypingPreset::Colemak).unwrap() {
            assert_eq!(e.draft().unwrap()["base"][&change.key], change.action);
        }
        assert_eq!(e.draft().unwrap()["fn"], original["fn"]);
        for usage in [0x39, 0xe1, 0x28] {
            assert_eq!(
                e.draft().unwrap()["base"][&format!("slot-{usage}")],
                Action::Key(0x29)
            );
        }
        e.revert().unwrap();
        assert_eq!(e.draft(), Some(&original));
        assert!(!e.dirty());
    }

    #[test]
    fn failure_recovery_and_readback_mismatch_retain_preset_draft() {
        let d = descriptor();
        for recovery in [
            Recovery::Verified,
            Recovery::Failed,
            Recovery::NotAttempted,
            Recovery::Unverified,
        ] {
            let mut e = editor(d.clone());
            e.edit_batch(plan(&d, "base", TypingPreset::Dvorak).unwrap())
                .unwrap();
            let draft = e.draft().cloned();
            e.request_apply().unwrap();
            e.accept_apply(Err(ApplyFailure {
                message: "write failed".into(),
                recovery: recovery.clone(),
            }));
            assert_eq!(
                e.problem(),
                Some(&Problem::Apply(ApplyFailure {
                    message: "write failed".into(),
                    recovery
                }))
            );
            assert_eq!(e.draft(), draft.as_ref());
            assert!(e.dirty());
            assert!(e.request_apply().is_err());
            e.accept_read(Ok(state(&d)));
            assert_eq!(e.status(), &Status::Ready);
            assert_eq!(e.draft(), draft.as_ref());
            e.request_apply().unwrap();
            e.accept_apply(Ok(state(&d)));
            assert_eq!(e.problem(), Some(&Problem::ApplyReadbackMismatch));
            assert_eq!(e.draft(), draft.as_ref());
        }
    }

    fn completion(command: &Command, result: FeatureResult<State>) -> Completion {
        Completion {
            generation: command.generation,
            operation: command.operation,
            payload: CompletionPayload::Keymap(result),
        }
    }

    #[test]
    fn session_success_accepts_submitted_preset_and_retains_newer_intent() {
        let d = descriptor();
        let mut session = Session::new(d.clone()).unwrap();
        assert!(
            session
                .stage_typing_preset("base", TypingPreset::Dvorak)
                .is_err()
        );
        session.connect().unwrap();
        let read = session.read().unwrap();
        assert_eq!(
            session.accept(completion(&read, FeatureResult::Read(Ok(state(&d))))),
            Outcome::Loaded
        );
        let before = session.keymap().draft().cloned();
        assert!(
            session
                .stage_typing_preset("fn", TypingPreset::Dvorak)
                .is_err()
        );
        assert_eq!(session.keymap().draft(), before.as_ref());
        session
            .stage_typing_preset("base", TypingPreset::Dvorak)
            .unwrap();
        let save = session.save().unwrap();
        let submitted = session.keymap().submitted().unwrap().clone();
        session
            .stage_typing_preset("base", TypingPreset::Colemak)
            .unwrap();
        let newer = session.keymap().draft().unwrap().clone();
        assert_eq!(session.keymap().submitted(), Some(&submitted));
        let mut readback = state(&d);
        readback.bindings = submitted.clone();
        readback.revision = vec![2];
        assert_eq!(
            session.accept(completion(&save, FeatureResult::Apply(Ok(readback)))),
            Outcome::Saved
        );
        assert_eq!(session.keymap().baseline().unwrap().bindings, submitted);
        assert_eq!(session.keymap().draft(), Some(&newer));
        assert!(session.keymap().dirty());
    }

    #[test]
    fn old_serialized_keys_default_to_unknown_position() {
        let mut key = serde_json::to_value(&descriptor().keys[0]).unwrap();
        key.as_object_mut().unwrap().remove("position");
        let key: PhysicalKey = serde_json::from_value(key).unwrap();
        assert_eq!(key.position, None);
    }
}
