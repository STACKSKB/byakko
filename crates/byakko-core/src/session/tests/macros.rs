use super::*;
use crate::macros::{self, library::Occupancy};
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
    s.disconnect();
    assert!(s.macros().unwrap().dirty());
    assert!(
        matches!(s.macros().unwrap().status(), macros::editor::Status::Unverified { problem: Problem::Apply(value) } if value == &failure)
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
            problem: AssignmentProblem::Device(Problem::Apply(failure))
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
            problem: AssignmentProblem::Device(Problem::Apply(failure))
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
        matches!(s.keymap().status(), crate::keymap::Status::Unverified { problem: Problem::Apply(value) } if value == &failure)
    );
    assert_eq!(
        s.macro_library().unwrap().occupancy("one"),
        Some(&Occupancy::Configured)
    );
}
