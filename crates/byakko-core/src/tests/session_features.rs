use super::*;
use crate::model::SnapshotEvidence;

fn configured() -> Session {
    session()
        .with_lighting(lighting::Capabilities {
            backend_id: "test".into(),
            effects: vec![
                lighting::Effect {
                    id: "steady".into(),
                    label: "Steady".into(),
                    brightness: Some(0..=100),
                    speed: None,
                    color: None,
                    options: vec![
                        lighting::Choice {
                            id: "one".into(),
                            label: "One".into(),
                        },
                        lighting::Choice {
                            id: "two".into(),
                            label: "Two".into(),
                        },
                    ],
                },
                lighting::Effect {
                    id: "other".into(),
                    label: "Other".into(),
                    brightness: None,
                    speed: None,
                    color: None,
                    options: vec![],
                },
            ],
            host_modes: vec![],
        })
        .unwrap()
        .with_picture(picture::Capabilities {
            backend_id: "test".into(),
            keys: vec!["a".into()],
            lighting_effect: Some("steady".into()),
        })
        .unwrap()
        .with_settings(settings::Capabilities {
            backend_id: "test".into(),
            fields: vec![settings::Field {
                id: "enabled".into(),
                label: "Enabled".into(),
                kind: settings::Kind::Toggle,
            }],
        })
        .unwrap()
}

#[test]
fn lighting_observation_allows_unrelated_keymap_edits() {
    let mut s = configured();
    s.connect().unwrap();
    let initial = s.read().unwrap();
    s.accept(result(
        &initial,
        FeatureResult::Read(Ok(state(Action::Key(4)))),
    ));
    let observation = s.observe(crate::contract::Feature::Lighting).unwrap();
    edit(&mut s);
    assert!(!s.blocks_editing());
    assert_eq!(
        s.accept(completion(
            &observation,
            CompletionPayload::Lighting(FeatureResult::Read(Ok(light(
                10,
                "one",
                SnapshotEvidence::Readback
            ))))
        )),
        Outcome::LightingLoaded
    );
    assert!(s.keymap().dirty());
}
fn light(level: u16, option: &str, evidence: SnapshotEvidence) -> lighting::Snapshot {
    lighting::Snapshot {
        backend_id: "test".into(),
        revision: vec![level as u8],
        picture_context: vec![1, if option == "one" { 1 } else { 2 }],
        evidence,
        content: lighting::Content::Editable(lighting::Setting {
            effect: "steady".into(),
            brightness: Some(level),
            speed: None,
            option: Some(option.into()),
            color: None,
        }),
    }
}
fn picture_snapshot() -> picture::Snapshot {
    picture::Snapshot {
        backend_id: "test".into(),
        revision: vec![1],
        context_revision: vec![1, 1],
        evidence: SnapshotEvidence::Readback,
        content: picture::Content::Editable([("a".into(), [1, 2, 3])].into()),
    }
}
fn completion(command: &Command, payload: CompletionPayload) -> Completion {
    Completion {
        generation: command.generation,
        operation: command.operation,
        payload,
    }
}
fn loaded_features() -> Session {
    let mut s = configured();
    s.connect().unwrap();
    let command = s.read_lighting().unwrap();
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Read(Ok(light(
                10,
                "one",
                SnapshotEvidence::Readback
            ))))
        )),
        Outcome::LightingLoaded
    );
    let command = s.read_picture().unwrap();
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Picture(FeatureResult::Read(Ok(picture_snapshot())))
        )),
        Outcome::PictureLoaded
    );
    s
}
#[test]
fn lighting_submission_retains_newer_intent_and_unrelated_picture() {
    let mut s = loaded_features();
    s.edit_lighting(lighting::Edit::Brightness(20)).unwrap();
    let command = s.save_lighting().unwrap();
    s.edit_lighting(lighting::Edit::Brightness(30)).unwrap();
    assert!(
        s.edit_picture(picture::Edit::Color {
            key: "a".into(),
            color: [9; 3]
        })
        .is_err()
    );
    assert!(s.read_lighting().is_err());
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Apply(Ok(light(
                20,
                "one",
                SnapshotEvidence::TransportAccepted
            ))))
        )),
        Outcome::LightingSaved
    );
    assert_eq!(s.lighting().unwrap().draft().unwrap().brightness, Some(30));
    assert!(s.lighting().unwrap().dirty());
    assert_eq!(s.picture().unwrap().status(), &Status::Ready);
    assert!(!s.busy());
    assert_eq!(s.save_lighting().unwrap().operation, command.operation + 1);
}
#[test]
fn dirty_picture_prevents_selector_changes_but_allows_same_layer_controls() {
    let mut s = loaded_features();
    s.edit_picture(picture::Edit::Color {
        key: "a".into(),
        color: [9; 3],
    })
    .unwrap();
    let before = s.picture().unwrap().draft().cloned();
    assert!(
        s.edit_lighting(lighting::Edit::Option("two".into()))
            .is_err()
    );
    assert!(
        s.edit_lighting(lighting::Edit::Effect("other".into()))
            .is_err()
    );
    let mut target = s.lighting().unwrap().draft().unwrap().clone();
    target.option = Some("two".into());
    assert!(s.stage_lighting(target).is_err());
    s.edit_lighting(lighting::Edit::Option("one".into()))
        .unwrap();
    s.edit_lighting(lighting::Edit::Effect("steady".into()))
        .unwrap();
    s.edit_lighting(lighting::Edit::Brightness(20)).unwrap();
    let command = s.save_lighting().unwrap();
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Apply(Ok(light(
                20,
                "one",
                SnapshotEvidence::TransportAccepted
            ))))
        )),
        Outcome::LightingSaved
    );
    assert_eq!(s.picture().unwrap().draft(), before.as_ref());
    assert!(s.picture().unwrap().dirty());
    assert!(s.save_picture().is_ok());
}

#[test]
fn picture_only_client_uses_its_snapshot_context_without_a_lighting_read() {
    let mut s = configured();
    s.connect().unwrap();
    let command = s.read_picture().unwrap();
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Picture(FeatureResult::Read(Ok(picture_snapshot())))
        )),
        Outcome::PictureLoaded
    );
    assert!(s.lighting().unwrap().baseline().is_none());
    s.edit_picture(picture::Edit::Color {
        key: "a".into(),
        color: [9; 3],
    })
    .unwrap();
    let command = s.save_picture().unwrap();
    assert_eq!(command.operation, 2);
    let CommandPayload::Picture(FeatureCommand::Apply { expected, .. }) = command.payload else {
        panic!("Expected picture save")
    };
    assert_eq!(expected, picture_snapshot());
}

#[test]
fn picture_upload_allows_a_known_matching_context_outside_its_display_effect() {
    let mut s = configured();
    s.connect().unwrap();
    let command = s.read_lighting().unwrap();
    let mut snapshot = light(10, "one", SnapshotEvidence::Readback);
    snapshot.content = lighting::Content::Editable(
        crate::editor::lighting::default_setting(s.lighting().unwrap().capabilities(), "other")
            .unwrap(),
    );
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Read(Ok(snapshot)))
        )),
        Outcome::LightingLoaded
    );
    let command = s.read_picture().unwrap();
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Picture(FeatureResult::Read(Ok(picture_snapshot())))
        )),
        Outcome::PictureLoaded
    );
    s.edit_picture(picture::Edit::Color {
        key: "a".into(),
        color: [9; 3],
    })
    .unwrap();
    assert!(s.save_picture().is_ok());
}

#[test]
fn selected_layer_must_be_loaded_before_painting_or_importing() {
    let mut s = loaded_features();
    s.edit_lighting(lighting::Edit::Option("two".into()))
        .unwrap();
    let paint = picture::Edit::Color {
        key: "a".into(),
        color: [9; 3],
    };
    assert!(s.edit_picture(paint.clone()).is_err());
    assert!(s.stage_picture_snapshot(&picture_snapshot()).is_err());
    assert!(s.save_picture().is_err());
    let command = s.save_lighting().unwrap();
    assert!(s.edit_picture(paint.clone()).is_err());
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Apply(Ok(light(
                10,
                "two",
                SnapshotEvidence::TransportAccepted
            ))))
        )),
        Outcome::LightingSaved
    );
    assert!(s.edit_picture(paint.clone()).is_err());
    let command = s.read_picture().unwrap();
    let mut target = picture_snapshot();
    target.context_revision = vec![1, 2];
    target.revision = vec![2];
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Picture(FeatureResult::Read(Ok(target.clone())))
        )),
        Outcome::PictureLoaded
    );
    s.edit_picture(paint).unwrap();
    let command = s.save_picture().unwrap();
    let CommandPayload::Picture(FeatureCommand::Apply { expected, .. }) = command.payload else {
        panic!("Expected picture save")
    };
    assert_eq!(expected, target);
}

#[test]
fn failed_layer_selection_retains_intent_and_rejects_old_picture_writes() {
    let mut s = loaded_features();
    let before = s.picture().unwrap().baseline().cloned();
    s.edit_lighting(lighting::Edit::Option("two".into()))
        .unwrap();
    let command = s.save_lighting().unwrap();
    let failure = ApplyFailure {
        message: "Disconnected".into(),
        recovery: Recovery::NotAttempted,
    };
    assert!(matches!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Apply(Err(failure)))
        )),
        Outcome::Failed(_)
    ));
    assert_eq!(
        s.lighting().unwrap().draft().unwrap().option.as_deref(),
        Some("two")
    );
    assert_eq!(s.picture().unwrap().baseline(), before.as_ref());
    assert!(
        s.edit_picture(picture::Edit::Color {
            key: "a".into(),
            color: [9; 3]
        })
        .is_err()
    );
    assert!(s.save_picture().is_err());
}
#[test]
fn selector_save_invalidates_picture_only_after_correlated_success() {
    let mut s = loaded_features();
    s.edit_lighting(lighting::Edit::Option("two".into()))
        .unwrap();
    let command = s.save_lighting().unwrap();
    assert_eq!(s.picture().unwrap().status(), &Status::Ready);
    let mut stale = completion(
        &command,
        CompletionPayload::Lighting(FeatureResult::Apply(Ok(light(
            10,
            "two",
            SnapshotEvidence::TransportAccepted,
        )))),
    );
    stale.generation += 1;
    assert_eq!(s.accept(stale), Outcome::Ignored);
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Picture(FeatureResult::Apply(Ok(picture_snapshot())))
        )),
        Outcome::Ignored
    );
    assert!(s.busy());
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Apply(Ok(light(
                10,
                "two",
                SnapshotEvidence::TransportAccepted
            ))))
        )),
        Outcome::LightingSaved
    );
    assert_eq!(s.picture().unwrap().problem(), Some(&Problem::ReadRequired));
    assert_eq!(s.picture().unwrap().draft().unwrap()["a"], [1, 2, 3]);
}
#[test]
fn failed_lighting_save_retains_newer_draft_and_typed_recovery() {
    let mut s = loaded_features();
    s.edit_lighting(lighting::Edit::Brightness(20)).unwrap();
    let command = s.save_lighting().unwrap();
    s.edit_lighting(lighting::Edit::Brightness(30)).unwrap();
    let failure = ApplyFailure {
        message: "failed".into(),
        recovery: Recovery::Verified,
    };
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Apply(Err(failure.clone())))
        )),
        Outcome::Failed(Problem::Apply(failure.clone()))
    );
    assert_eq!(s.lighting().unwrap().draft().unwrap().brightness, Some(30));
    assert_eq!(
        s.lighting().unwrap().baseline(),
        Some(&light(10, "one", SnapshotEvidence::Readback))
    );
    s.disconnect().unwrap();
    assert_eq!(
        s.lighting().unwrap().problem(),
        Some(&Problem::Apply(failure))
    );
}
#[test]
fn picture_import_is_atomic_and_apply_accepts_transport_evidence() {
    let mut s = loaded_features();
    let mut target = picture_snapshot();
    target.content = picture::Content::Editable([("a".into(), [9; 3])].into());
    target.context_revision = vec![2];
    assert!(s.stage_picture_snapshot(&target).is_err());
    assert!(!s.picture().unwrap().dirty());
    target.context_revision = vec![1, 1];
    s.stage_picture_snapshot(&target).unwrap();
    let command = s.save_picture().unwrap();
    s.edit_picture(picture::Edit::Color {
        key: "a".into(),
        color: [8; 3],
    })
    .unwrap();
    target.evidence = SnapshotEvidence::TransportAccepted;
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Picture(FeatureResult::Apply(Ok(target)))
        )),
        Outcome::PictureSaved
    );
    assert_eq!(s.picture().unwrap().draft().unwrap()["a"], [8; 3]);
    assert!(s.picture().unwrap().dirty());
}
#[test]
fn settings_route_whole_baseline_and_single_scalar_with_newer_intent() {
    let mut s = configured();
    s.connect().unwrap();
    let baseline = settings::Snapshot {
        backend_id: "test".into(),
        revision: vec![1, 99],
        content: settings::Content::Editable(
            [("enabled".into(), settings::Value::Toggle(false))].into(),
        ),
    };
    let read = s.read_settings().unwrap();
    assert_eq!(
        s.accept(completion(
            &read,
            CompletionPayload::Settings(FeatureResult::Read(Ok(baseline.clone())))
        )),
        Outcome::SettingsLoaded
    );
    s.edit_settings(settings::Edit {
        id: "enabled".into(),
        value: settings::Value::Toggle(true),
    })
    .unwrap();
    let command = s.save_settings().unwrap();
    assert!(
        matches!(&command.payload, CommandPayload::Settings(FeatureCommand::Apply { expected, desired }) if expected == &baseline && desired.id == "enabled")
    );
    s.edit_settings(settings::Edit {
        id: "enabled".into(),
        value: settings::Value::Toggle(false),
    })
    .unwrap();
    let observed = settings::Snapshot {
        content: settings::Content::Editable(
            [("enabled".into(), settings::Value::Toggle(true))].into(),
        ),
        ..baseline
    };
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Settings(FeatureResult::Apply(Ok(observed)))
        )),
        Outcome::SettingsSaved
    );
    assert!(s.settings().unwrap().dirty());
    s.revert_settings().unwrap();
    assert!(!s.settings().unwrap().dirty());
}

#[test]
fn initial_and_conflicting_lighting_observations_use_actual_picture_context() {
    let mut s = configured();
    s.connect().unwrap();
    let command = s.read_picture().unwrap();
    s.accept(completion(
        &command,
        CompletionPayload::Picture(FeatureResult::Read(Ok(picture_snapshot()))),
    ));
    let command = s.read_lighting().unwrap();
    let mut opaque = light(10, "one", SnapshotEvidence::Readback);
    opaque.content = lighting::Content::Opaque {
        reason: "unsupported parameter".into(),
    };
    // Losing the editable projection does not change the actual selector.
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Read(Ok(opaque)))
        )),
        Outcome::LightingLoaded
    );
    assert_eq!(s.picture().unwrap().status(), &Status::Ready);
    let command = s.read_lighting().unwrap();
    s.accept(completion(
        &command,
        CompletionPayload::Lighting(FeatureResult::Read(Ok(light(
            10,
            "two",
            SnapshotEvidence::Readback,
        )))),
    ));
    assert_eq!(s.picture().unwrap().problem(), Some(&Problem::ReadRequired));

    let mut s = loaded_features();
    s.edit_lighting(lighting::Edit::Brightness(20)).unwrap();
    let command = s.read_lighting().unwrap();
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Read(Ok(light(
                10,
                "two",
                SnapshotEvidence::Readback
            ))))
        )),
        Outcome::Conflict
    );
    assert_eq!(s.picture().unwrap().problem(), Some(&Problem::ReadRequired));
    assert_eq!(s.lighting().unwrap().draft().unwrap().brightness, Some(20));
}

#[test]
fn independent_picture_and_invalid_lighting_observations_keep_cached_picture() {
    let mut s = loaded_features();
    let command = s.read_picture().unwrap();
    let mut independent = picture_snapshot();
    independent.context_revision.clear();
    s.accept(completion(
        &command,
        CompletionPayload::Picture(FeatureResult::Read(Ok(independent))),
    ));
    s.edit_lighting(lighting::Edit::Option("two".into()))
        .unwrap();
    let command = s.save_lighting().unwrap();
    s.accept(completion(
        &command,
        CompletionPayload::Lighting(FeatureResult::Apply(Ok(light(
            10,
            "two",
            SnapshotEvidence::TransportAccepted,
        )))),
    ));
    assert_eq!(s.picture().unwrap().status(), &Status::Ready);

    let mut s = loaded_features();
    let command = s.read_lighting().unwrap();
    let mut invalid = light(10, "two", SnapshotEvidence::Readback);
    invalid.backend_id = "another".into();
    assert!(matches!(
        s.accept(completion(
            &command,
            CompletionPayload::Lighting(FeatureResult::Read(Ok(invalid)))
        )),
        Outcome::Failed(Problem::Read(_))
    ));
    assert_eq!(s.picture().unwrap().status(), &Status::Ready);
}

fn preparation_session() -> Session {
    let mut caps = configured().lighting().unwrap().capabilities().clone();
    let mut effect = caps.effects[0].clone();
    effect.id = "picture".into();
    caps.effects.push(effect);
    session()
        .with_lighting(caps)
        .unwrap()
        .with_picture(picture::Capabilities {
            backend_id: "test".into(),
            keys: vec!["a".into()],
            lighting_effect: Some("picture".into()),
        })
        .unwrap()
}
fn continued(outcome: Outcome) -> Command {
    let Outcome::Continue(command) = outcome else {
        panic!("expected next preparation command, got {outcome:?}");
    };
    command
}
fn activated(command: &Command) -> lighting::Snapshot {
    let CommandPayload::Lighting(FeatureCommand::Apply { desired, .. }) = &command.payload else {
        panic!("expected lighting setter");
    };
    lighting::Snapshot {
        backend_id: "test".into(),
        revision: vec![2],
        picture_context: vec![2, 1],
        evidence: SnapshotEvidence::TransportAccepted,
        content: lighting::Content::Editable(desired.clone()),
    }
}
#[test]
fn picture_preparation_reads_once_activates_then_reads_picture_without_lighting_getter() {
    let mut s = preparation_session();
    s.connect().unwrap();
    let read = s.prepare_picture().unwrap().unwrap();
    assert!(matches!(
        read.payload,
        CommandPayload::Lighting(FeatureCommand::Read(()))
    ));
    let save = continued(s.accept(completion(
        &read,
        CompletionPayload::Lighting(FeatureResult::Read(Ok(light(
            10,
            "one",
            SnapshotEvidence::Readback,
        )))),
    )));
    assert!(s.edit_lighting(lighting::Edit::Brightness(30)).is_err());
    assert!(s.prepare_picture().is_err());
    let picture_read = continued(s.accept(completion(
        &save,
        CompletionPayload::Lighting(FeatureResult::Apply(Ok(activated(&save)))),
    )));
    assert!(matches!(
        picture_read.payload,
        CommandPayload::Picture(FeatureCommand::Read(()))
    ));
    let mut observed = picture_snapshot();
    observed.context_revision = vec![2, 1];
    assert_eq!(
        s.accept(completion(
            &picture_read,
            CompletionPayload::Picture(FeatureResult::Read(Ok(observed)))
        )),
        Outcome::PictureLoaded
    );
    assert_eq!(picture_read.operation, read.operation + 2);
    assert!(!s.busy());
    assert!(s.prepare_picture().unwrap().is_none());
    s.edit_picture(picture::Edit::Color {
        key: "a".into(),
        color: [9; 3],
    })
    .unwrap();
    assert!(s.prepare_picture().unwrap().is_none());
}
#[test]
fn picture_preparation_reports_applied_lighting_when_picture_read_fails() {
    let mut s = preparation_session();
    s.connect().unwrap();
    let read = s.prepare_picture().unwrap().unwrap();
    let save = continued(s.accept(completion(
        &read,
        CompletionPayload::Lighting(FeatureResult::Read(Ok(light(
            10,
            "one",
            SnapshotEvidence::Readback,
        )))),
    )));
    let picture_read = continued(s.accept(completion(
        &save,
        CompletionPayload::Lighting(FeatureResult::Apply(Ok(activated(&save)))),
    )));
    assert_eq!(
        s.accept(completion(
            &picture_read,
            CompletionPayload::Picture(FeatureResult::Read(Err("read failed".into())))
        )),
        Outcome::PicturePreparationFailed {
            lighting_applied: true,
            problem: WorkflowProblem::Device(Problem::Read("read failed".into()))
        }
    );
    assert_eq!(s.lighting().unwrap().draft().unwrap().effect, "picture");
    assert!(!s.lighting().unwrap().dirty());
    assert!(s.prepare_picture().is_err());
    assert!(!s.busy());
}
#[test]
fn preparation_never_clears_failures_or_staged_intent() {
    let mut s = loaded_features();
    s.edit_lighting(lighting::Edit::Brightness(20)).unwrap();
    assert!(s.prepare_picture().is_err());
    assert!(!s.busy());
    s.revert_lighting().unwrap();
    let read = s.read_lighting().unwrap();
    s.accept(completion(
        &read,
        CompletionPayload::Lighting(FeatureResult::Read(Err("failed".into()))),
    ));
    assert!(s.prepare_picture().is_err());
    assert_eq!(
        s.lighting().unwrap().problem(),
        Some(&Problem::Read("failed".into()))
    );
    assert!(!s.busy());
}

#[test]
fn independent_picture_preparation_does_not_require_lighting() {
    let mut s = session()
        .with_picture(picture::Capabilities {
            backend_id: "test".into(),
            keys: vec!["a".into()],
            lighting_effect: None,
        })
        .unwrap();
    s.connect().unwrap();
    let command = s.prepare_picture().unwrap().unwrap();
    assert!(matches!(
        command.payload,
        CommandPayload::Picture(FeatureCommand::Read(()))
    ));
    let mut observed = picture_snapshot();
    observed.context_revision.clear();
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Picture(FeatureResult::Read(Ok(observed)))
        )),
        Outcome::PictureLoaded
    );
    assert!(s.lighting().is_none());
    assert!(!s.busy());
    assert!(s.prepare_picture().unwrap().is_none());
    s.edit_picture(picture::Edit::Color {
        key: "a".into(),
        color: [9; 3],
    })
    .unwrap();
    assert!(s.prepare_picture().unwrap().is_none());
    assert_eq!(s.operation, command.operation);
}

fn refresh_session() -> Session {
    configured()
        .with_macros(crate::model::macros::Capabilities {
            backend_id: "test".into(),
            slots: vec![crate::model::macros::Choice {
                id: "one".into(),
                label: "One".into(),
            }],
            repeat_counts: 0..=10,
            editable_repeat_counts: 1..=10,
            delays_ms: 0..=100,
            keys: Some(4..=10),
            buttons: vec![],
            movement: None,
            backend_actions: vec![],
            byte_budget: None,
            bindings: vec![],
        })
        .unwrap()
}
fn settings_snapshot() -> settings::Snapshot {
    settings::Snapshot {
        backend_id: "test".into(),
        revision: vec![1],
        content: settings::Content::Editable(
            [("enabled".into(), settings::Value::Toggle(false))].into(),
        ),
    }
}
fn macro_snapshot() -> crate::model::macros::Snapshot {
    crate::model::macros::Snapshot {
        backend_id: "test".into(),
        slot: "one".into(),
        revision: vec![1],
        content: crate::model::macros::Content::Editable(crate::model::macros::Program {
            repeat_count: 1,
            events: vec![],
        }),
    }
}
fn accept_refresh(s: &mut Session, command: &Command) -> Outcome {
    let payload = match &command.payload {
        CommandPayload::Keymap(FeatureCommand::Read(())) => {
            CompletionPayload::Keymap(FeatureResult::Read(Ok(state(Action::Key(4)))))
        }
        CommandPayload::Lighting(FeatureCommand::Read(())) => CompletionPayload::Lighting(
            FeatureResult::Read(Ok(light(10, "one", SnapshotEvidence::Readback))),
        ),
        CommandPayload::Settings(FeatureCommand::Read(())) => {
            CompletionPayload::Settings(FeatureResult::Read(Ok(settings_snapshot())))
        }
        CommandPayload::Picture(FeatureCommand::Read(())) => {
            CompletionPayload::Picture(FeatureResult::Read(Ok(picture_snapshot())))
        }
        CommandPayload::Macro(FeatureCommand::Read(slot)) => CompletionPayload::Macro {
            slot: slot.clone(),
            result: FeatureResult::Read(Ok(macro_snapshot())),
        },
        _ => panic!("refresh must contain only a feature read"),
    };
    s.accept(completion(command, payload))
}
fn fully_loaded() -> Session {
    let mut s = refresh_session();
    s.connect().unwrap();
    while let Some(command) = s.refresh_next().unwrap() {
        accept_refresh(&mut s, &command);
    }
    let command = s.read_macro().unwrap();
    accept_refresh(&mut s, &command);
    s
}
#[test]
fn refresh_reads_each_supported_feature_once_and_only_previously_loaded_selected_macro() {
    let mut s = refresh_session();
    assert!(s.refresh_next().is_err());
    s.connect().unwrap();
    assert!(!s.requires_manual_read());
    for outcome in [
        Outcome::Loaded,
        Outcome::LightingLoaded,
        Outcome::SettingsLoaded,
        Outcome::PictureLoaded,
    ] {
        let command = s.refresh_next().unwrap().unwrap();
        assert!(s.refresh_next().is_err());
        assert_eq!(accept_refresh(&mut s, &command), outcome);
    }
    assert!(s.refresh_next().unwrap().is_none());
    assert!(s.macros().unwrap().baseline().is_none());
    let command = s.read_macro().unwrap();
    accept_refresh(&mut s, &command);
    s.disconnect().unwrap();
    s.connect().unwrap();
    for outcome in [
        Outcome::Loaded,
        Outcome::LightingLoaded,
        Outcome::SettingsLoaded,
        Outcome::PictureLoaded,
        Outcome::MacroLoaded,
    ] {
        let command = s.refresh_next().unwrap().unwrap();
        assert_eq!(accept_refresh(&mut s, &command), outcome);
    }
    let operation = s.operation;
    assert!(s.refresh_next().unwrap().is_none());
    assert!(s.refresh_next().unwrap().is_none());
    assert_eq!(s.operation, operation);
    assert!(!s.requires_manual_read());
}
#[test]
fn refresh_retains_dirty_drafts_and_conflicts_when_cached_before_image_changes() {
    let mut s = fully_loaded();
    edit(&mut s);
    s.edit_lighting(lighting::Edit::Brightness(20)).unwrap();
    s.edit_picture(picture::Edit::Color {
        key: "a".into(),
        color: [9; 3],
    })
    .unwrap();
    s.edit_settings(settings::Edit {
        id: "enabled".into(),
        value: settings::Value::Toggle(true),
    })
    .unwrap();
    s.edit_macro(crate::model::macros::Edit::Repeat(2)).unwrap();
    s.disconnect().unwrap();
    s.connect().unwrap();
    while let Some(command) = s.refresh_next().unwrap() {
        assert!(!matches!(
            accept_refresh(&mut s, &command),
            Outcome::Conflict | Outcome::Failed(_)
        ));
    }
    assert!(s.keymap().dirty());
    assert_eq!(s.lighting().unwrap().draft().unwrap().brightness, Some(20));
    assert_eq!(s.picture().unwrap().draft().unwrap()["a"], [9; 3]);
    assert!(s.settings().unwrap().dirty());
    assert_eq!(s.macros().unwrap().draft().unwrap().repeat_count, 2);
    s.disconnect().unwrap();
    s.connect().unwrap();
    let command = s.refresh_next().unwrap().unwrap();
    let mut changed = state(Action::Key(4));
    changed.revision = vec![99];
    assert_eq!(
        s.accept(completion(
            &command,
            CompletionPayload::Keymap(FeatureResult::Read(Ok(changed)))
        )),
        Outcome::Conflict
    );
    assert!(s.requires_manual_read());
    let status = s.keymap().status().clone();
    s.disconnect().unwrap();
    s.connect().unwrap();
    assert_eq!(s.keymap().status(), &status);
    assert!(s.requires_manual_read());
}
#[test]
fn all_feature_failed_write_cautions_survive_disconnect_and_connect() {
    for feature in 0..5 {
        let mut s = fully_loaded();
        let command = match feature {
            0 => {
                edit(&mut s);
                s.save().unwrap()
            }
            1 => {
                s.edit_macro(crate::model::macros::Edit::Repeat(2)).unwrap();
                s.save_macro().unwrap()
            }
            2 => {
                s.edit_lighting(lighting::Edit::Brightness(20)).unwrap();
                s.save_lighting().unwrap()
            }
            3 => {
                s.edit_picture(picture::Edit::Color {
                    key: "a".into(),
                    color: [9; 3],
                })
                .unwrap();
                s.save_picture().unwrap()
            }
            _ => {
                s.edit_settings(settings::Edit {
                    id: "enabled".into(),
                    value: settings::Value::Toggle(true),
                })
                .unwrap();
                s.save_settings().unwrap()
            }
        };
        let failure = ApplyFailure {
            message: "write failed".into(),
            recovery: Recovery::Verified,
        };
        let payload = match feature {
            0 => CompletionPayload::Keymap(FeatureResult::Apply(Err(failure.clone()))),
            1 => CompletionPayload::Macro {
                slot: "one".into(),
                result: FeatureResult::Apply(Err(failure.clone())),
            },
            2 => CompletionPayload::Lighting(FeatureResult::Apply(Err(failure.clone()))),
            3 => CompletionPayload::Picture(FeatureResult::Apply(Err(failure.clone()))),
            _ => CompletionPayload::Settings(FeatureResult::Apply(Err(failure.clone()))),
        };
        assert_eq!(
            s.accept(completion(&command, payload)),
            Outcome::Failed(Problem::Apply(failure.clone()))
        );
        assert!(s.requires_manual_read());
        s.disconnect().unwrap();
        s.connect().unwrap();
        assert!(s.requires_manual_read());
        let problem = match feature {
            0 => s.keymap().problem(),
            1 => s.macros().unwrap().problem(),
            2 => s.lighting().unwrap().problem(),
            3 => s.picture().unwrap().problem(),
            _ => s.settings().unwrap().problem(),
        };
        assert_eq!(problem, Some(&Problem::Apply(failure)));
    }
}
#[test]
fn manual_read_caution_distinguishes_write_trust_from_read_failures() {
    let mut s = loaded_features();
    let command = s.read_lighting().unwrap();
    s.accept(completion(
        &command,
        CompletionPayload::Lighting(FeatureResult::Read(Err("read failed".into()))),
    ));
    assert!(!s.requires_manual_read());
    for problem in [
        Problem::InvalidApplyResult("invalid".into()),
        Problem::ApplyReadbackMismatch,
    ] {
        assert!(requires_manual_read::<State>(&Status::Unverified {
            problem
        }));
    }
}
