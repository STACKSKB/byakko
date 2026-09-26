use super::*;
use crate::{library::macros::Occupancy, model::macros};
fn caps() -> macros::Capabilities {
    macros::Capabilities {
        backend_id: "test".into(),
        slots: ["one", "two", "opaque"]
            .map(|id| macros::Choice {
                id: id.into(),
                label: id.into(),
            })
            .into(),
        repeat_counts: 0..=4,
        editable_repeat_counts: 1..=4,
        delays_ms: 0..=100,
        keys: Some(1..=10),
        buttons: vec![],
        movement: None,
        backend_actions: vec![],
        byte_budget: None,
        bindings: ["one", "two", "opaque"]
            .map(|slot| macros::Binding {
                slot: slot.into(),
                id: "play".into(),
                label: "Play".into(),
                action: Action::Named {
                    id: format!("play-{slot}"),
                },
                required_repeat_count: None,
            })
            .into(),
    }
}
fn snapshot(slot: &str, configured: bool) -> macros::Snapshot {
    macros::Snapshot {
        backend_id: "test".into(),
        slot: slot.into(),
        revision: vec![1],
        content: macros::Content::Editable(macros::Program {
            repeat_count: 1,
            events: if configured {
                vec![macros::Event {
                    action: macros::Action::Key {
                        usage: 4,
                        pressed: true,
                    },
                    delay_ms: 0,
                }]
            } else {
                vec![]
            },
        }),
    }
}
fn completion(
    command: &Command,
    slot: &str,
    result: FeatureResult<macros::Snapshot>,
) -> Completion {
    Completion {
        generation: command.generation,
        operation: command.operation,
        payload: CompletionPayload::Macro {
            slot: slot.into(),
            result,
        },
    }
}
fn catalog(command: &Command, result: Result<Vec<macros::Snapshot>, String>) -> Completion {
    Completion {
        generation: command.generation,
        operation: command.operation,
        payload: CompletionPayload::ReadMacroCatalog { result },
    }
}
fn loaded_macro() -> Session {
    let mut s = loaded().with_macros(caps()).unwrap();
    let c = s.read_macro().unwrap();
    assert_eq!(
        s.accept(completion(
            &c,
            "one",
            FeatureResult::Read(Ok(snapshot("one", true)))
        )),
        Outcome::MacroLoaded
    );
    s
}
fn successful_macro_save(command: &Command) -> macros::Snapshot {
    let CommandPayload::Macro(FeatureCommand::Apply { expected, desired }) = &command.payload
    else {
        panic!("macro save without preflight")
    };
    macros::Snapshot {
        revision: vec![2],
        content: macros::Content::Editable(desired.clone()),
        ..expected.clone()
    }
}

fn document() -> macros::Document {
    macros::Document {
        format_version: 2,
        backend_id: "another-backend".into(),
        source_slot: "unrelated-source-slot".into(),
        name: "Greeting".into(),
        binding: Some("suggested-source-binding".into()),
        program: macros::Program {
            repeat_count: 2,
            events: vec![macros::Event {
                action: macros::Action::Key {
                    usage: 5,
                    pressed: true,
                },
                delay_ms: 10,
            }],
        },
    }
}
#[test]
fn portable_document_stages_selected_slot_without_using_source_metadata_as_targets() {
    let mut s = loaded_macro();
    let baseline = s.macros().unwrap().baseline().cloned();
    let original_keymap = s.keymap().draft().cloned();
    let imported = document();
    let metadata = s.stage_macro_document(&imported).unwrap();
    assert_eq!(
        metadata,
        macros::DocumentMetadata {
            name: imported.name.clone(),
            binding: imported.binding.clone()
        }
    );
    assert_eq!(s.macros().unwrap().slot(), "one");
    assert_eq!(s.macros().unwrap().baseline(), baseline.as_ref());
    assert_eq!(s.macros().unwrap().draft(), Some(&imported.program));
    assert_eq!(s.keymap().draft(), original_keymap.as_ref());
    assert!(s.macros().unwrap().dirty());
    assert!(!s.busy());
    let exported = s
        .export_macro_document(metadata.name, metadata.binding)
        .unwrap();
    assert_eq!(exported.format_version, 2);
    assert_eq!(exported.backend_id, "test");
    assert_eq!(exported.source_slot, "one");
    assert_eq!(exported.program, imported.program);
    let command = s.save_macro().unwrap();
    assert!(s.stage_macro_document(&document()).is_err());
    assert_eq!(
        s.accept(completion(
            &command,
            "one",
            FeatureResult::Apply(Ok(successful_macro_save(&command)))
        )),
        Outcome::MacroSaved
    );
}
#[test]
fn portable_document_rejection_is_atomic_and_zero_is_exportable_but_not_staged() {
    let mut s = loaded_macro();
    let original = s.macros().unwrap().draft().cloned();
    let mut invalid = document();
    invalid.format_version = 1;
    assert!(s.stage_macro_document(&invalid).is_err());
    invalid = document();
    invalid.backend_id.clear();
    assert!(s.stage_macro_document(&invalid).is_err());
    invalid = document();
    invalid.source_slot.clear();
    assert!(s.stage_macro_document(&invalid).is_err());
    invalid = document();
    invalid.program.repeat_count = 0;
    assert!(s.stage_macro_document(&invalid).is_err());
    invalid = document();
    invalid.program.events[0].action = macros::Action::Backend {
        backend_id: "another-backend".into(),
        id: "unknown".into(),
        pressed: true,
    };
    assert!(s.stage_macro_document(&invalid).is_err());
    assert_eq!(s.macros().unwrap().draft(), original.as_ref());
    s.revert_macro().unwrap();
    let mut raw_zero = snapshot("one", false);
    let macros::Content::Editable(program) = &mut raw_zero.content else {
        unreachable!()
    };
    program.repeat_count = 0;
    let read = s.read_macro().unwrap();
    assert_eq!(
        s.accept(completion(&read, "one", FeatureResult::Read(Ok(raw_zero)))),
        Outcome::MacroLoaded
    );
    let exported = s.export_macro_document("Legacy".into(), None).unwrap();
    assert_eq!(exported.program.repeat_count, 0);
    assert!(s.stage_macro_document(&exported).is_err());
    assert!(!s.macros().unwrap().dirty());
    s.select_macro("opaque").unwrap();
    let read = s.read_macro().unwrap();
    let opaque = macros::Snapshot {
        content: macros::Content::Opaque {
            reason: "preserved".into(),
        },
        ..snapshot("opaque", false)
    };
    s.accept(completion(&read, "opaque", FeatureResult::Read(Ok(opaque))));
    assert!(s.stage_macro_document(&document()).is_err());
    assert!(s.export_macro_document("Opaque".into(), None).is_err());
}
fn successful_keymap_save(command: &Command) -> State {
    let CommandPayload::Keymap(FeatureCommand::Apply { expected, desired }) = &command.payload
    else {
        panic!("assignment without preflight")
    };
    let mut actual = expected.clone();
    actual.revision = vec![2];
    for change in desired {
        actual
            .bindings
            .get_mut(&change.layer)
            .unwrap()
            .insert(change.key.clone(), change.action.clone());
    }
    actual
}
#[test]
fn passive_discovery_and_foreground_reads_have_independent_tickets() {
    let mut s = loaded().with_macros(caps()).unwrap();
    assert_eq!(
        s.macro_library().unwrap().occupancy("two"),
        Some(&Occupancy::Unknown)
    );
    let scan = s.request_macro_catalog().unwrap();
    assert!(!s.busy());
    assert!(s.catalog_scanning());
    let read = s.read_macro().unwrap();
    let wrong = completion(&read, "two", FeatureResult::Read(Ok(snapshot("two", true))));
    assert_eq!(s.accept(wrong), Outcome::Ignored);
    assert!(s.busy());
    assert_eq!(
        s.accept(completion(
            &read,
            "one",
            FeatureResult::Read(Ok(snapshot("one", true)))
        )),
        Outcome::MacroLoaded
    );
    let mut opaque = snapshot("opaque", false);
    opaque.content = macros::Content::Opaque {
        reason: "raw".into(),
    };
    assert_eq!(
        s.accept(catalog(
            &scan,
            Ok(vec![snapshot("one", false), snapshot("two", false), opaque])
        )),
        Outcome::CatalogLoaded
    );
    let library = s.macro_library().unwrap();
    assert_eq!(library.occupancy("one"), Some(&Occupancy::Configured));
    assert_eq!(library.occupancy("two"), Some(&Occupancy::Empty));
    assert_eq!(library.occupancy("opaque"), Some(&Occupancy::Opaque));
    assert_eq!(s.macro_candidate().unwrap(), "two");
    s.select_macro("two").unwrap();
    assert!(s.macros().unwrap().baseline().is_none());
    let read = s.read_macro().unwrap();
    assert!(
        matches!(read.payload, CommandPayload::Macro(FeatureCommand::Read(ref slot)) if slot == "two")
    );
}
#[test]
fn malformed_catalog_is_atomic_and_cancel_does_not_settle_foreground() {
    let mut s = loaded_macro();
    let scan = s.request_macro_catalog().unwrap();
    assert!(matches!(
        s.accept(catalog(
            &scan,
            Ok(vec![
                snapshot("two", false),
                snapshot("two", false),
                snapshot("opaque", false)
            ])
        )),
        Outcome::CatalogFailed(_)
    ));
    assert_eq!(
        s.macro_library().unwrap().occupancy("two"),
        Some(&Occupancy::Unknown)
    );
    let scan = s.request_macro_catalog().unwrap();
    let read = s.read_macro().unwrap();
    s.cancel_catalog();
    assert_eq!(
        s.accept(catalog(&scan, Err("cancelled".into()))),
        Outcome::Ignored
    );
    assert!(s.busy());
    assert_eq!(
        s.accept(completion(
            &read,
            "one",
            FeatureResult::Read(Ok(snapshot("one", true)))
        )),
        Outcome::MacroLoaded
    );
}
#[test]
fn macro_failure_preserves_draft_and_recovery_across_disconnect() {
    let mut s = loaded_macro();
    s.edit_macro(macros::Edit::Repeat(2)).unwrap();
    let c = s.save_macro().unwrap();
    let failure = ApplyFailure {
        message: "failure".into(),
        recovery: Recovery::Failed,
    };
    assert_eq!(
        s.accept(completion(
            &c,
            "one",
            FeatureResult::Apply(Err(failure.clone()))
        )),
        Outcome::Failed(Problem::Apply(failure.clone()))
    );
    s.disconnect().unwrap();
    assert!(s.macros().unwrap().dirty());
    assert!(
        matches!(s.macros().unwrap().status(), crate::editor::Status::Unverified { problem: Problem::Apply(value) } if value == &failure)
    );
}
#[test]
fn save_and_assignment_are_ordered_with_explicit_partial_failure() {
    let mut s = loaded_macro();
    s.edit_macro(macros::Edit::Repeat(2)).unwrap();
    let scan = s.request_macro_catalog().unwrap();
    let save = s.save_and_assign_macro("base", "a", "play").unwrap();
    assert!(!s.catalog_scanning());
    let actual = successful_macro_save(&save);
    let Outcome::Continue(assign) =
        s.accept(completion(&save, "one", FeatureResult::Apply(Ok(actual))))
    else {
        panic!("must assign after saving")
    };
    assert_eq!(
        s.accept(catalog(&scan, Err("cancelled".into()))),
        Outcome::Ignored
    );
    assert!(s.busy());
    assert!(!s.macros().unwrap().dirty());
    let failure = ApplyFailure {
        message: "assignment failed".into(),
        recovery: Recovery::Verified,
    };
    assert_eq!(
        s.accept(result(&assign, FeatureResult::Apply(Err(failure.clone())))),
        Outcome::AssignmentFailed {
            macro_saved: true,
            problem: WorkflowProblem::Device(Problem::Apply(failure))
        }
    );
    assert!(s.keymap().dirty());
    assert_eq!(
        s.macro_library().unwrap().occupancy("one"),
        Some(&Occupancy::Configured)
    );
}
#[test]
fn clean_macro_assigns_native_binding_without_resave_or_descriptor_choice() {
    let mut s = loaded_macro();
    let assign = s.save_and_assign_macro("base", "a", "play").unwrap();
    assert!(matches!(assign.payload, CommandPayload::Keymap(_)));
    let actual = successful_keymap_save(&assign);
    assert_eq!(
        s.accept(result(&assign, FeatureResult::Apply(Ok(actual)))),
        Outcome::AssignmentSucceeded { macro_saved: false }
    );
    assert_eq!(
        s.keymap().baseline().unwrap().bindings["base"]["a"],
        Action::Named {
            id: "play-one".into()
        }
    );
    assert_eq!(s.macro_candidate().unwrap(), "two");
}
#[test]
fn assignment_rejects_unrelated_key_edits_and_invalid_target_before_macro_save() {
    let mut s = loaded_macro();
    s.edit_macro(macros::Edit::Repeat(2)).unwrap();
    assert!(s.save_and_assign_macro("base", "missing", "play").is_err());
    assert!(!s.busy());
    assert!(s.macros().unwrap().dirty());
    edit(&mut s);
    assert!(s.save_and_assign_macro("base", "a", "play").is_err());
    assert!(!s.busy());
    assert!(s.keymap().dirty());
}
#[test]
fn import_is_one_atomic_revision_bound_edit_and_zero_count_stays_read_only() {
    let mut s = loaded_macro();
    let before = s.macros().unwrap().draft().cloned();
    let mut target = snapshot("two", true);
    assert!(s.stage_macro_snapshot(&target).is_err());
    target.slot = "one".into();
    target.revision = vec![9];
    assert!(s.stage_macro_snapshot(&target).is_err());
    target.revision = vec![1];
    let macros::Content::Editable(program) = &mut target.content else {
        unreachable!()
    };
    program.repeat_count = 0;
    assert!(s.stage_macro_snapshot(&target).is_err());
    assert_eq!(s.macros().unwrap().draft(), before.as_ref());
    let macros::Content::Editable(program) = &mut target.content else {
        unreachable!()
    };
    program.repeat_count = 2;
    s.stage_macro_snapshot(&target).unwrap();
    assert!(s.macros().unwrap().dirty());
    s.revert_macro().unwrap();
    assert!(!s.macros().unwrap().dirty());
}
#[test]
fn wrong_snapshot_slot_is_failure_without_replacing_selected_baseline() {
    let mut s = loaded_macro();
    let original = s.macros().unwrap().baseline().cloned();
    let read = s.read_macro().unwrap();
    assert!(matches!(
        s.accept(completion(
            &read,
            "one",
            FeatureResult::Read(Ok(snapshot("two", false)))
        )),
        Outcome::Failed(Problem::Read(_))
    ));
    assert_eq!(s.macros().unwrap().baseline(), original.as_ref());
    assert!(!s.busy());
}
#[test]
fn saved_macro_then_verified_assignment_finishes_without_another_read() {
    let mut s = loaded_macro();
    s.edit_macro(macros::Edit::Repeat(2)).unwrap();
    let save = s.save_and_assign_macro("base", "a", "play").unwrap();
    let actual = successful_macro_save(&save);
    let Outcome::Continue(assign) =
        s.accept(completion(&save, "one", FeatureResult::Apply(Ok(actual))))
    else {
        panic!("next effect must assign")
    };
    let actual = successful_keymap_save(&assign);
    assert_eq!(
        s.accept(result(&assign, FeatureResult::Apply(Ok(actual)))),
        Outcome::AssignmentSucceeded { macro_saved: true }
    );
    assert!(!s.keymap().dirty());
    assert!(!s.macros().unwrap().dirty());
    assert!(!s.busy());
}
#[test]
fn failed_macro_save_never_stages_assignment() {
    let mut s = loaded_macro();
    s.edit_macro(macros::Edit::Repeat(2)).unwrap();
    let save = s.save_and_assign_macro("base", "a", "play").unwrap();
    let failure = ApplyFailure {
        message: "macro failed".into(),
        recovery: Recovery::Verified,
    };
    assert_eq!(
        s.accept(completion(
            &save,
            "one",
            FeatureResult::Apply(Err(failure.clone()))
        )),
        Outcome::AssignmentFailed {
            macro_saved: false,
            problem: WorkflowProblem::Device(Problem::Apply(failure))
        }
    );
    assert!(!s.keymap().dirty());
    assert!(s.macros().unwrap().dirty());
    assert!(!s.busy());
}
#[test]
fn add_prefers_known_empty_over_unknown_and_reads_the_candidate() {
    let mut s = loaded().with_macros(caps()).unwrap();
    assert_eq!(s.macro_candidate().unwrap(), "one");
    s.select_macro("two").unwrap();
    let read = s.read_macro().unwrap();
    assert_eq!(
        s.accept(completion(
            &read,
            "two",
            FeatureResult::Read(Ok(snapshot("two", false)))
        )),
        Outcome::MacroLoaded
    );
    assert_eq!(s.macro_candidate().unwrap(), "two");
    assert_eq!(
        s.macro_library().unwrap().occupancy("one"),
        Some(&Occupancy::Unknown)
    );
}
#[test]
fn uncertain_macro_save_forgets_only_affected_occupancy() {
    for recovery in [
        Recovery::Failed,
        Recovery::Unverified,
        Recovery::Verified,
        Recovery::NotAttempted,
    ] {
        let mut s = loaded_macro();
        let scan = s.request_macro_catalog().unwrap();
        let mut opaque = snapshot("opaque", false);
        opaque.content = macros::Content::Opaque {
            reason: "raw".into(),
        };
        assert_eq!(
            s.accept(catalog(
                &scan,
                Ok(vec![snapshot("one", false), snapshot("two", false), opaque])
            )),
            Outcome::CatalogLoaded
        );
        let read = s.read_macro().unwrap();
        assert_eq!(
            s.accept(completion(
                &read,
                "one",
                FeatureResult::Read(Ok(snapshot("one", false)))
            )),
            Outcome::MacroLoaded
        );
        let macros::Content::Editable(program) = snapshot("one", true).content else {
            unreachable!()
        };
        s.edit_macro(macros::Edit::Insert {
            at: 0,
            event: program.events[0].clone(),
        })
        .unwrap();
        let save = s.save_macro().unwrap();
        let failure = ApplyFailure {
            message: "write failed".into(),
            recovery: recovery.clone(),
        };
        assert_eq!(
            s.accept(completion(
                &save,
                "one",
                FeatureResult::Apply(Err(failure.clone()))
            )),
            Outcome::Failed(Problem::Apply(failure))
        );
        let expected = if matches!(recovery, Recovery::Failed | Recovery::Unverified) {
            Occupancy::Unknown
        } else {
            Occupancy::Empty
        };
        assert_eq!(s.macro_library().unwrap().occupancy("one"), Some(&expected));
        assert_eq!(
            s.macro_library().unwrap().occupancy("two"),
            Some(&Occupancy::Empty)
        );
        assert_eq!(
            s.macro_library().unwrap().occupancy("opaque"),
            Some(&Occupancy::Opaque)
        );
        assert!(s.macros().unwrap().dirty());
        if expected == Occupancy::Unknown {
            assert_eq!(s.macro_candidate().unwrap(), "two");
        }
    }
}
#[test]
fn conflict_updates_observed_occupancy_and_stale_catalog_cannot_undo_it() {
    let mut s = loaded().with_macros(caps()).unwrap();
    let read = s.read_macro().unwrap();
    assert_eq!(
        s.accept(completion(
            &read,
            "one",
            FeatureResult::Read(Ok(snapshot("one", false)))
        )),
        Outcome::MacroLoaded
    );
    s.edit_macro(macros::Edit::Repeat(2)).unwrap();
    let scan = s.request_macro_catalog().unwrap();
    let read = s.read_macro().unwrap();
    let observed = snapshot("one", true);
    assert_eq!(
        s.accept(completion(&read, "one", FeatureResult::Read(Ok(observed)))),
        Outcome::Conflict
    );
    assert_eq!(
        s.macro_library().unwrap().occupancy("one"),
        Some(&Occupancy::Configured)
    );
    assert_eq!(
        s.accept(catalog(
            &scan,
            Ok(vec![
                snapshot("one", false),
                snapshot("two", false),
                snapshot("opaque", false)
            ])
        )),
        Outcome::CatalogLoaded
    );
    assert_eq!(
        s.macro_library().unwrap().occupancy("one"),
        Some(&Occupancy::Configured)
    );
    assert!(s.macros().unwrap().dirty());
}
#[test]
fn failed_foreground_write_cancels_passive_ticket_and_retains_failure() {
    let mut s = loaded_macro();
    let scan = s.request_macro_catalog().unwrap();
    edit(&mut s);
    let save = s.save().unwrap();
    let failure = ApplyFailure {
        message: "keymap write failed".into(),
        recovery: Recovery::Failed,
    };
    assert_eq!(
        s.accept(result(&save, FeatureResult::Apply(Err(failure.clone())))),
        Outcome::Failed(Problem::Apply(failure.clone()))
    );
    assert!(!s.catalog_scanning());
    assert_eq!(
        s.accept(catalog(&scan, Err("invalidated by write".into()))),
        Outcome::Ignored
    );
    assert!(
        matches!(s.keymap().status(), crate::editor::Status::Unverified { problem: Problem::Apply(value) } if value == &failure)
    );
    assert_eq!(
        s.macro_library().unwrap().occupancy("one"),
        Some(&Occupancy::Configured)
    );
}
#[test]
fn rejected_foreground_allocation_clears_only_the_attempted_submission() {
    let mut s = loaded_macro();
    s.edit_macro(macros::Edit::Repeat(2)).unwrap();
    edit(&mut s);
    let command = s.save().unwrap();
    assert!(s.keymap().submitted().is_some());
    assert!(s.save_macro().is_err());
    assert!(s.keymap().submitted().is_some());
    assert!(s.macros().unwrap().submitted().is_none());
    assert!(s.macros().unwrap().dirty());
    let actual = successful_keymap_save(&command);
    assert_eq!(
        s.accept(result(&command, FeatureResult::Apply(Ok(actual)))),
        Outcome::Saved
    );
}
#[test]
fn exhausted_operation_identity_leaves_no_phantom_submission() {
    let mut s = loaded_macro();
    edit(&mut s);
    s.operation = u64::MAX;
    assert!(s.save().is_err());
    assert!(s.keymap().submitted().is_none());
    assert!(s.keymap().dirty());
    assert!(!s.busy());
}
#[test]
fn recording_is_exclusive_local_activity_and_stale_results_do_not_edit_it() {
    use crate::recorder::macros::{DelayPolicy, StopOutcome, Transition};
    let mut s = loaded_macro()
        .with_lighting(crate::model::lighting::Capabilities {
            backend_id: "test".into(),
            effects: vec![crate::model::lighting::Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: Some(0..=100),
                speed: None,
                options: vec![],
                color: None,
            }],
            host_modes: vec![],
        })
        .unwrap()
        .with_picture(crate::model::picture::Capabilities {
            backend_id: "test".into(),
            keys: vec!["a".into()],
            lighting_effect: None,
        })
        .unwrap()
        .with_settings(crate::model::settings::Capabilities {
            backend_id: "test".into(),
            fields: vec![crate::model::settings::Field {
                id: "a".into(),
                label: "A".into(),
                kind: crate::model::settings::Kind::Toggle,
            }],
        })
        .unwrap()
        .with_archive(crate::model::archive::ArchiveCapabilities {
            backend_id: "test".into(),
            format_id: "native".into(),
            max_bytes: 4,
        })
        .unwrap();
    let read = s.read_macro().unwrap();
    assert!(s.start_recording(DelayPolicy::Fixed(5)).is_err());
    assert_eq!(
        s.accept(completion(
            &read,
            "one",
            FeatureResult::Read(Ok(snapshot("one", true)))
        )),
        Outcome::MacroLoaded
    );
    let scan = s.request_macro_catalog().unwrap();
    assert!(s.start_recording(DelayPolicy::Fixed(5)).is_err());
    s.cancel_catalog();
    s.start_recording(DelayPolicy::Measured { terminal_ms: 5 })
        .unwrap();
    assert!(s.recording());
    assert!(!s.busy());
    assert!(!s.catalog_scanning());
    assert!(s.start_recording(DelayPolicy::Fixed(5)).is_err());
    assert!(s.read().is_err());
    assert!(s.read_macro().is_err());
    assert!(s.capture_archive().is_err());
    assert!(s.export_macro_document("Name".into(), None).is_err());
    assert!(s.read_lighting().is_err());
    assert!(s.read_picture().is_err());
    assert!(s.read_settings().is_err());
    assert!(
        s.edit_lighting(crate::model::lighting::Edit::Brightness(1))
            .is_err()
    );
    assert!(
        s.edit_picture(crate::model::picture::Edit::Color {
            key: "a".into(),
            color: [1; 3]
        })
        .is_err()
    );
    assert!(
        s.edit_settings(crate::model::settings::Edit {
            id: "a".into(),
            value: crate::model::settings::Value::Toggle(true)
        })
        .is_err()
    );
    assert!(s.save().is_err());
    assert!(s.save_macro().is_err());
    assert!(s.connect().is_err());
    assert!(s.request_macro_catalog().is_err());
    assert!(
        s.edit(Change {
            layer: "base".into(),
            key: "a".into(),
            action: Action::Key(5)
        })
        .is_err()
    );
    assert!(s.edit_macro(macros::Edit::Repeat(2)).is_err());
    assert!(s.revert().is_err());
    assert!(s.revert_macro().is_err());
    assert!(s.select_macro("two").is_err());
    assert!(s.stage_macro_snapshot(&snapshot("one", true)).is_err());
    assert!(s.stage_macro_document(&document()).is_err());
    assert!(s.save_and_assign_macro("base", "a", "play").is_err());
    assert!(s.keymap().submitted().is_none());
    assert!(s.macros().unwrap().submitted().is_none());
    let baseline = s.macros().unwrap().baseline().cloned();
    assert_eq!(
        s.accept(catalog(&scan, Err("cancelled".into()))),
        Outcome::Ignored
    );
    assert_eq!(s.macros().unwrap().baseline(), baseline.as_ref());
    assert_eq!(
        s.record_input(
            macros::Action::Key {
                usage: 5,
                pressed: true
            },
            100
        )
        .unwrap(),
        Transition::Recorded
    );
    assert_eq!(
        s.record_input(
            macros::Action::Key {
                usage: 5,
                pressed: true
            },
            101
        )
        .unwrap(),
        Transition::Duplicate
    );
    assert_eq!(s.stop_recording(125).unwrap(), StopOutcome::Complete);
    assert!(!s.recording());
    assert!(!s.busy());
    let program = s.macros().unwrap().draft().unwrap();
    assert_eq!(program.events.len(), 3);
    assert_eq!(program.events[1].delay_ms, 25);
    assert_eq!(
        program.events[2].action,
        macros::Action::Key {
            usage: 5,
            pressed: false
        }
    );
    assert_eq!(program.events[2].delay_ms, 5);
    assert!(s.macros().unwrap().dirty());
}
#[test]
fn recording_start_rejects_unverified_opaque_and_zero_count_programs() {
    use crate::recorder::macros::DelayPolicy;
    let mut s = loaded().with_macros(caps()).unwrap();
    assert!(s.start_recording(DelayPolicy::Fixed(5)).is_err());
    let read = s.read_macro().unwrap();
    let mut zero = snapshot("one", false);
    let macros::Content::Editable(program) = &mut zero.content else {
        unreachable!()
    };
    program.repeat_count = 0;
    assert_eq!(
        s.accept(completion(&read, "one", FeatureResult::Read(Ok(zero)))),
        Outcome::MacroLoaded
    );
    assert!(s.start_recording(DelayPolicy::Fixed(5)).is_err());
    assert!(!s.recording());
    assert!(!s.macros().unwrap().dirty());
    let read = s.read_macro().unwrap();
    let mut opaque = snapshot("one", false);
    opaque.content = macros::Content::Opaque {
        reason: "raw".into(),
    };
    assert_eq!(
        s.accept(completion(&read, "one", FeatureResult::Read(Ok(opaque)))),
        Outcome::MacroLoaded
    );
    assert!(s.start_recording(DelayPolicy::Fixed(5)).is_err());
}
#[test]
fn rejected_recording_edge_preserves_capacity_for_stop_releases() {
    use crate::recorder::macros::{DelayPolicy, StopOutcome};
    let mut capabilities = caps();
    capabilities.byte_budget = Some(macros::ByteBudget {
        limit: 4,
        overhead: 0,
        key: 2,
        button: 2,
        movement: 2,
        backend: 2,
        inline_delays: 0..=100,
        extended_delay: 1,
    });
    let mut s = loaded().with_macros(capabilities).unwrap();
    let read = s.read_macro().unwrap();
    assert_eq!(
        s.accept(completion(
            &read,
            "one",
            FeatureResult::Read(Ok(snapshot("one", false)))
        )),
        Outcome::MacroLoaded
    );
    s.start_recording(DelayPolicy::Measured { terminal_ms: 5 })
        .unwrap();
    s.record_input(
        macros::Action::Key {
            usage: 4,
            pressed: true,
        },
        100,
    )
    .unwrap();
    let before = s.macros().unwrap().draft().cloned();
    assert!(
        s.record_input(
            macros::Action::Key {
                usage: 5,
                pressed: true
            },
            110
        )
        .is_err()
    );
    assert!(s.recording());
    assert_eq!(s.macros().unwrap().draft(), before.as_ref());
    assert!(
        s.record_input(
            macros::Action::Key {
                usage: 4,
                pressed: false
            },
            90
        )
        .is_err()
    );
    assert_eq!(s.macros().unwrap().draft(), before.as_ref());
    assert_eq!(s.stop_recording(1000).unwrap(), StopOutcome::TimingClamped);
    assert!(!s.recording());
    let program = s.macros().unwrap().draft().unwrap();
    assert_eq!(program.events.len(), 2);
    assert_eq!(program.events[0].delay_ms, 0);
    assert_eq!(
        program.events[1].action,
        macros::Action::Key {
            usage: 4,
            pressed: false
        }
    );
}

#[test]
fn disconnect_finishes_reserved_releases_at_last_supplied_timestamp() {
    use crate::recorder::macros::{DelayPolicy, StopOutcome};
    let mut s = loaded_macro();
    s.start_recording(DelayPolicy::Measured { terminal_ms: 5 })
        .unwrap();
    s.record_input(
        macros::Action::Key {
            usage: 5,
            pressed: true,
        },
        100,
    )
    .unwrap();
    s.record_input(
        macros::Action::Key {
            usage: 6,
            pressed: true,
        },
        120,
    )
    .unwrap();
    assert_eq!(s.disconnect().unwrap(), Some(StopOutcome::Complete));
    assert!(!s.recording());
    assert!(!s.busy());
    assert_eq!(s.connection(), &Connection::Disconnected);
    let program = s.macros().unwrap().draft().unwrap();
    assert_eq!(program.events.len(), 5);
    assert_eq!(program.events[1].delay_ms, 20);
    assert_eq!(program.events[2].delay_ms, 0);
    assert_eq!(
        program.events[3].action,
        macros::Action::Key {
            usage: 6,
            pressed: false
        }
    );
    assert_eq!(
        program.events[4].action,
        macros::Action::Key {
            usage: 5,
            pressed: false
        }
    );
    assert_eq!(program.events[4].delay_ms, 5);
    assert!(s.macros().unwrap().dirty());
    assert_eq!(s.disconnect().unwrap(), None);
}
