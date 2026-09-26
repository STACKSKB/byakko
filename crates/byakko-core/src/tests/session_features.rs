use super::*;
use crate::model::SnapshotEvidence;

fn configured() -> Session {
    session()
        .with_lighting(lighting::Capabilities {
            backend_id: "test".into(),
            effects: vec![lighting::Effect {
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
            }],
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
