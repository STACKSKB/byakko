use super::*;
use crate::{
    contract::Recovery,
    editor::{lighting::LightingRules, picture::PictureRules, settings::SettingsRules},
    model::{SnapshotEvidence, lighting, picture, settings},
};
fn lighting_editor() -> Editor<LightingRules> {
    Editor::new(
        LightingRules::new(lighting::Capabilities {
            backend_id: "test".into(),
            effects: vec![lighting::Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: Some(0..=100),
                speed: None,
                options: vec![],
                color: None,
            }],
            host_modes: vec![],
        })
        .unwrap(),
    )
}
fn lighting_snapshot(level: u16, evidence: SnapshotEvidence) -> lighting::Snapshot {
    lighting::Snapshot {
        backend_id: "test".into(),
        revision: vec![level as u8],
        evidence,
        content: lighting::Content::Editable(lighting::Setting {
            effect: "steady".into(),
            brightness: Some(level),
            speed: None,
            option: None,
            color: None,
        }),
    }
}
#[test]
fn completed_submission_advances_baseline_and_retains_newer_intent() {
    let mut editor = lighting_editor();
    editor.accept_read(Ok(lighting_snapshot(10, SnapshotEvidence::Readback)));
    editor.edit(lighting::Edit::Brightness(20)).unwrap();
    let (expected, submitted) = editor.request_apply().unwrap();
    assert_eq!(expected.revision, vec![10]);
    assert_eq!(submitted.brightness, Some(20));
    editor.edit(lighting::Edit::Brightness(30)).unwrap();
    editor.accept_apply(Ok(lighting_snapshot(
        20,
        SnapshotEvidence::TransportAccepted,
    )));
    assert_eq!(editor.status(), &Status::Ready);
    assert_eq!(editor.baseline().unwrap().revision, vec![20]);
    assert_eq!(editor.draft().unwrap().brightness, Some(30));
    assert!(editor.dirty());
    assert!(editor.submitted().is_none());
    assert_eq!(editor.request_apply().unwrap().1.brightness, Some(30));
}
#[test]
fn failure_and_invalid_result_retain_newest_draft_baseline_and_typed_recovery() {
    let mut editor = lighting_editor();
    let baseline = lighting_snapshot(10, SnapshotEvidence::Readback);
    editor.accept_read(Ok(baseline.clone()));
    editor.edit(lighting::Edit::Brightness(20)).unwrap();
    editor.request_apply().unwrap();
    editor.edit(lighting::Edit::Brightness(30)).unwrap();
    let failure = ApplyFailure {
        message: "write failed".into(),
        recovery: Recovery::Failed,
    };
    editor.accept_apply(Err(failure.clone()));
    editor.invalidate();
    assert_eq!(
        editor.status(),
        &Status::Unverified {
            problem: Problem::Apply(failure)
        }
    );
    assert_eq!(editor.baseline(), Some(&baseline));
    assert_eq!(editor.draft().unwrap().brightness, Some(30));
    assert!(editor.submitted().is_none());
    editor.accept_read(Ok(baseline.clone()));
    editor.request_apply().unwrap();
    let mut wrong = lighting_snapshot(30, SnapshotEvidence::Readback);
    wrong.backend_id = "other".into();
    editor.accept_apply(Ok(wrong));
    assert!(matches!(
        editor.status(),
        Status::Unverified {
            problem: Problem::InvalidApplyResult(_)
        }
    ));
    assert_eq!(editor.baseline(), Some(&baseline));
    assert_eq!(editor.draft().unwrap().brightness, Some(30));
}
#[test]
fn lighting_read_requires_observation_but_setter_accepts_transport_evidence() {
    let mut editor = lighting_editor();
    editor.accept_read(Ok(lighting_snapshot(
        10,
        SnapshotEvidence::TransportAccepted,
    )));
    assert!(editor.baseline().is_none());
    assert!(matches!(
        editor.status(),
        Status::Unverified {
            problem: Problem::Read(_)
        }
    ));
    editor.accept_read(Ok(lighting_snapshot(10, SnapshotEvidence::Readback)));
    editor.edit(lighting::Edit::Brightness(20)).unwrap();
    editor.request_apply().unwrap();
    editor.accept_apply(Ok(lighting_snapshot(
        20,
        SnapshotEvidence::TransportAccepted,
    )));
    assert_eq!(editor.status(), &Status::Ready);
    assert!(!editor.dirty());
    assert_eq!(
        editor.baseline().unwrap().evidence,
        SnapshotEvidence::TransportAccepted
    );
}
#[test]
fn picture_retains_selector_context_and_distinguishes_read_from_setter_evidence() {
    let mut editor = Editor::new(PictureRules::new(picture::Capabilities {
        backend_id: "test".into(),
        keys: vec!["a".into()],
        lighting_effect: None,
    }));
    let baseline = picture::Snapshot {
        backend_id: "test".into(),
        revision: vec![1],
        context_revision: vec![9],
        evidence: SnapshotEvidence::Readback,
        content: picture::Content::Editable([("a".into(), [1, 2, 3])].into()),
    };
    let mut unobserved = baseline.clone();
    unobserved.evidence = SnapshotEvidence::TransportAccepted;
    editor.accept_read(Ok(unobserved));
    assert!(editor.baseline().is_none());
    editor.accept_read(Ok(baseline.clone()));
    editor
        .edit(picture::Edit::Color {
            key: "a".into(),
            color: [4, 5, 6],
        })
        .unwrap();
    let (expected, colors) = editor.request_apply().unwrap();
    assert_eq!(expected.context_revision, vec![9]);
    let accepted = picture::Snapshot {
        revision: vec![2],
        evidence: SnapshotEvidence::TransportAccepted,
        content: picture::Content::Editable(colors),
        ..baseline
    };
    editor.accept_apply(Ok(accepted.clone()));
    assert_eq!(editor.status(), &Status::Ready);
    assert!(!editor.dirty());
    editor.accept_read(Ok(accepted.clone()));
    assert_eq!(editor.baseline(), Some(&accepted));
    assert!(matches!(
        editor.status(),
        Status::Unverified {
            problem: Problem::Read(_)
        }
    ));
}
#[test]
fn settings_share_lifecycle_while_planning_exactly_one_scalar() {
    let capabilities = settings::Capabilities {
        backend_id: "test".into(),
        fields: ["a", "b"]
            .map(|id| settings::Field {
                id: id.into(),
                label: id.into(),
                kind: settings::Kind::Toggle,
            })
            .into(),
    };
    let mut editor = Editor::new(SettingsRules::new(capabilities).unwrap());
    let baseline = settings::Snapshot {
        backend_id: "test".into(),
        revision: vec![1, 255],
        content: settings::Content::Editable(
            [
                ("a".into(), settings::Value::Toggle(false)),
                ("b".into(), settings::Value::Toggle(false)),
            ]
            .into(),
        ),
    };
    editor.accept_read(Ok(baseline.clone()));
    let edit = settings::Edit {
        id: "a".into(),
        value: settings::Value::Toggle(true),
    };
    editor.edit(edit.clone()).unwrap();
    let staged = editor.draft().cloned();
    assert!(
        editor
            .edit(settings::Edit {
                id: "b".into(),
                value: settings::Value::Toggle(true)
            })
            .is_err()
    );
    assert_eq!(editor.draft(), staged.as_ref());
    let (expected, write) = editor.request_apply().unwrap();
    assert_eq!(expected, baseline);
    assert_eq!(write, edit);
    let observed = settings::Snapshot {
        revision: vec![2, 255],
        content: settings::Content::Editable(staged.unwrap()),
        ..baseline
    };
    editor.accept_apply(Ok(observed.clone()));
    assert!(!editor.dirty());
    let malformed = settings::Snapshot {
        content: settings::Content::Editable(Default::default()),
        ..observed.clone()
    };
    editor.accept_read(Ok(malformed));
    assert_eq!(editor.baseline(), Some(&observed));
    assert!(matches!(
        editor.status(),
        Status::Unverified {
            problem: Problem::Read(_)
        }
    ));
}
