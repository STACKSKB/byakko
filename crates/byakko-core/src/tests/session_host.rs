use super::*;
use crate::{
    contract::{HostEvent, HostEventKind, HostTicket},
    model::lighting::{self, Evidence},
    workflow::host::{HostOutcome, Phase},
};

fn setting(effect: &str, brightness: u16) -> lighting::Setting {
    lighting::Setting {
        effect: effect.into(),
        brightness: Some(brightness),
        speed: None,
        option: None,
        color: None,
    }
}
fn capabilities() -> lighting::Capabilities {
    lighting::Capabilities {
        backend_id: "test".into(),
        effects: vec![lighting::Effect {
            id: "steady".into(),
            label: "Steady".into(),
            brightness: Some(0..=5),
            speed: None,
            options: vec![],
            color: None,
        }],
        host_modes: vec![
            lighting::HostMode {
                id: "screen".into(),
                label: "Screen".into(),
                source: lighting::HostSource::ScreenAverage,
                requires_enabled_setting: None,
                parameters: None,
            },
            lighting::HostMode {
                id: "audio".into(),
                label: "Audio".into(),
                source: lighting::HostSource::PlaybackAudio { bands: 2 },
                requires_enabled_setting: None,
                parameters: Some(lighting::HostParameters {
                    schema: lighting::Effect {
                        id: "audio".into(),
                        label: "Audio parameters".into(),
                        brightness: Some(0..=3),
                        speed: None,
                        options: vec![],
                        color: None,
                    },
                    default: setting("audio", 2),
                }),
            },
        ],
    }
}
fn snapshot(evidence: Evidence) -> lighting::Snapshot {
    lighting::Snapshot {
        backend_id: "test".into(),
        revision: vec![2, 3],
        picture_context: vec![1, 1],
        evidence,
        content: lighting::Content::Editable(setting("steady", 2)),
    }
}
fn configured() -> Session {
    loaded().with_lighting(capabilities()).unwrap()
}
fn loaded_host() -> Session {
    let mut session = configured();
    let read = session.read_lighting().unwrap();
    assert_eq!(
        session.accept(Completion {
            generation: read.generation,
            operation: read.operation,
            payload: CompletionPayload::Lighting(FeatureResult::Read(Ok(snapshot(
                Evidence::Readback
            ))))
        }),
        Outcome::LightingLoaded
    );
    session
}
fn prerequisite_host() -> Session {
    let mut caps = capabilities();
    caps.host_modes[0].requires_enabled_setting = Some("backlight".into());
    let mut session = loaded().with_lighting(caps).unwrap();
    let read = session.read_lighting().unwrap();
    session.accept(Completion {
        generation: read.generation,
        operation: read.operation,
        payload: CompletionPayload::Lighting(FeatureResult::Read(Ok(snapshot(Evidence::Readback)))),
    });
    session
}
fn read_toggle(session: &mut Session, content: crate::model::settings::Content) {
    let read = session.read_settings().unwrap();
    session.accept(Completion {
        generation: read.generation,
        operation: read.operation,
        payload: CompletionPayload::Settings(FeatureResult::Read(Ok(
            crate::model::settings::Snapshot {
                backend_id: "test".into(),
                revision: vec![1],
                content,
            },
        ))),
    });
}
#[test]
fn host_prerequisite_uses_only_verified_advertised_toggle_baseline() {
    use crate::model::settings::{Capabilities, Content, Edit, Field, Kind, Value};
    let mut session = prerequisite_host();
    let operation = session.operation;
    assert!(session.start_host("screen", None).is_err());
    assert_eq!(session.operation, operation);
    let mut session = session
        .with_settings(Capabilities {
            backend_id: "test".into(),
            fields: vec![Field {
                id: "backlight".into(),
                label: "Backlight".into(),
                kind: Kind::Toggle,
            }],
        })
        .unwrap();
    assert!(
        session
            .start_host("screen", None)
            .unwrap_err()
            .contains("Backlight")
    );
    read_toggle(
        &mut session,
        Content::Editable([(String::from("backlight"), Value::Toggle(false))].into()),
    );
    session
        .edit_settings(Edit {
            id: "backlight".into(),
            value: Value::Toggle(true),
        })
        .unwrap();
    let operation = session.operation;
    let draft = session.settings().unwrap().draft().cloned();
    assert!(
        session
            .start_host("screen", None)
            .unwrap_err()
            .contains("Backlight")
    );
    assert_eq!(session.operation, operation);
    assert_eq!(session.settings().unwrap().draft(), draft.as_ref());
    session.revert_settings().unwrap();
    read_toggle(
        &mut session,
        Content::Editable([(String::from("backlight"), Value::Toggle(true))].into()),
    );
    // Unsaved disabling likewise does not replace the verified device baseline.
    session
        .edit_settings(Edit {
            id: "backlight".into(),
            value: Value::Toggle(false),
        })
        .unwrap();
    let start = session.start_host("screen", None).unwrap();
    session.accept_host(finished(
        start.ticket,
        Err(ApplyFailure {
            message: "not started".into(),
            recovery: Recovery::NotAttempted,
        }),
        None,
    ));
    session.disconnect().unwrap();
    session.connect().unwrap();
    let read = session.read_lighting().unwrap();
    session.accept(Completion {
        generation: read.generation,
        operation: read.operation,
        payload: CompletionPayload::Lighting(FeatureResult::Read(Ok(snapshot(Evidence::Readback)))),
    });
    let operation = session.operation;
    assert!(
        session
            .start_host("screen", None)
            .unwrap_err()
            .contains("Backlight")
    );
    assert_eq!(session.operation, operation);
    session.revert_settings().unwrap();
    read_toggle(
        &mut session,
        Content::Opaque {
            reason: "unknown setting".into(),
        },
    );
    assert!(session.start_host("screen", None).is_err());
}
fn event(ticket: HostTicket, kind: HostEventKind) -> HostEvent {
    HostEvent { ticket, kind }
}
fn finished(
    ticket: HostTicket,
    restored: Result<lighting::Snapshot, ApplyFailure>,
    problem: Option<String>,
) -> HostEvent {
    event(ticket, HostEventKind::Finished { restored, problem })
}
#[test]
fn start_requires_connected_clean_editable_lighting_and_exact_advertised_parameters() {
    let mut session = configured();
    assert!(session.start_host("screen", None).is_err());
    let mut session = loaded_host();
    let operation = session.operation;
    assert!(session.start_host("unknown", None).is_err());
    assert!(
        session
            .start_host("screen", Some(setting("steady", 2)))
            .is_err()
    );
    assert!(
        session
            .start_host("audio", Some(setting("audio", 4)))
            .is_err()
    );
    assert!(
        session
            .start_host("audio", Some(setting("steady", 2)))
            .is_err()
    );
    assert_eq!(session.operation, operation);
    session
        .edit_lighting(lighting::Edit::Brightness(3))
        .unwrap();
    assert!(session.start_host("screen", None).is_err());
    session.revert_lighting().unwrap();
    let pending = session.read_lighting().unwrap();
    assert!(session.start_host("screen", None).is_err());
    session.accept(Completion {
        generation: pending.generation,
        operation: pending.operation,
        payload: CompletionPayload::Lighting(FeatureResult::Read(Ok(snapshot(Evidence::Readback)))),
    });
    session.disconnect().unwrap();
    assert!(session.start_host("screen", None).is_err());
    session.connect().unwrap();
    assert!(session.start_host("screen", None).is_err());
    let mut session = loaded_host();
    let start = session.start_host("audio", None).unwrap();
    assert_eq!(start.setting, Some(setting("audio", 2)));
    assert_eq!(start.expected, snapshot(Evidence::Readback));
    assert_eq!(start.mode, capabilities().host_modes[1]);
    assert_eq!(start.ticket.operation, session.operation);
}
#[test]
fn host_freezes_other_activity_and_stop_during_startup_stays_stopping_on_late_started() {
    let mut session = loaded_host();
    let start = session.start_host("screen", None).unwrap();
    assert_eq!(session.host().phase(), Phase::Starting);
    assert_eq!(session.host().mode_id(), Some("screen"));
    assert_eq!(session.host().ticket(), Some(start.ticket));
    assert!(!session.busy());
    assert!(session.read().is_err());
    assert!(session.read_lighting().is_err());
    assert!(session.connect().is_err());
    assert!(
        session
            .edit_lighting(lighting::Edit::Brightness(3))
            .is_err()
    );
    assert!(
        session
            .edit(Change {
                layer: "base".into(),
                key: "a".into(),
                action: Action::Key(5)
            })
            .is_err()
    );
    assert!(session.request_macro_catalog().is_err());
    assert!(session.start_recording(DelayPolicy::Fixed(5)).is_err());
    assert!(session.refresh_next().is_err());
    assert!(session.start_host("screen", None).is_err());
    assert_eq!(session.stop_host(), Some(start.ticket));
    assert_eq!(session.stop_host(), Some(start.ticket));
    assert_eq!(session.host().phase(), Phase::Stopping);
    assert_eq!(
        session.accept_host(event(start.ticket, HostEventKind::Started)),
        HostOutcome::Stopping
    );
    assert_eq!(session.host().phase(), Phase::Stopping);
    assert_eq!(
        session.accept_host(event(start.ticket, HostEventKind::Started)),
        HostOutcome::Ignored
    );
    assert_eq!(
        session.accept_host(finished(
            start.ticket,
            Ok(snapshot(Evidence::Readback)),
            None
        )),
        HostOutcome::Finished
    );
    assert!(session.host().is_idle());
    assert_eq!(session.stop_host(), None);
    assert_eq!(
        session.accept_host(finished(
            start.ticket,
            Ok(snapshot(Evidence::Readback)),
            None
        )),
        HostOutcome::Ignored
    );
    assert_eq!(session.operation, start.ticket.operation);
    assert_eq!(session.lighting().unwrap().status(), &Status::Ready);
}
#[test]
fn restored_frame_failure_keeps_ready_and_upgrades_transport_baseline_to_readback() {
    let mut session = loaded_host();
    session
        .edit_lighting(lighting::Edit::Brightness(3))
        .unwrap();
    let save = session.save_lighting().unwrap();
    let mut accepted = snapshot(Evidence::TransportAccepted);
    accepted.content = lighting::Content::Editable(setting("steady", 3));
    accepted.revision = vec![3, 3];
    session.accept(Completion {
        generation: save.generation,
        operation: save.operation,
        payload: CompletionPayload::Lighting(FeatureResult::Apply(Ok(accepted.clone()))),
    });
    let start = session.start_host("screen", None).unwrap();
    assert_eq!(
        session.accept_host(event(start.ticket, HostEventKind::Started)),
        HostOutcome::Started
    );
    assert!(session.host().active());
    assert_eq!(
        session.accept_host(event(start.ticket, HostEventKind::Started)),
        HostOutcome::Ignored
    );
    let restored = lighting::Snapshot {
        evidence: Evidence::Readback,
        ..accepted
    };
    assert_eq!(
        session.accept_host(finished(
            start.ticket,
            Ok(restored.clone()),
            Some("frame failed".into())
        )),
        HostOutcome::Failed(ApplyFailure {
            message: "frame failed".into(),
            recovery: Recovery::Verified
        })
    );
    assert_eq!(session.lighting().unwrap().baseline(), Some(&restored));
    assert_eq!(session.lighting().unwrap().status(), &Status::Ready);
    assert!(!session.requires_manual_read());
}
#[test]
fn startup_rejection_preserves_known_baseline_only_for_not_attempted_or_verified_recovery() {
    for recovery in [
        Recovery::NotAttempted,
        Recovery::Verified,
        Recovery::Failed,
        Recovery::Unverified,
    ] {
        let mut session = loaded_host();
        let start = session.start_host("screen", None).unwrap();
        let failure = ApplyFailure {
            message: "start failed".into(),
            recovery: recovery.clone(),
        };
        assert_eq!(
            session.accept_host(finished(start.ticket, Err(failure.clone()), None)),
            HostOutcome::Failed(failure.clone())
        );
        assert!(session.host().is_idle());
        assert_eq!(
            session.lighting().unwrap().baseline(),
            Some(&snapshot(Evidence::Readback))
        );
        if matches!(recovery, Recovery::NotAttempted | Recovery::Verified) {
            assert_eq!(session.lighting().unwrap().status(), &Status::Ready);
            assert!(!session.requires_manual_read());
        } else {
            assert_eq!(
                session.lighting().unwrap().problem(),
                Some(&Problem::Apply(failure))
            );
            assert!(session.requires_manual_read());
        }
    }
}
#[test]
fn post_started_error_or_invalid_restoration_retains_typed_hold_and_original_draft() {
    for restoration in [
        Err(ApplyFailure {
            message: "restore failed".into(),
            recovery: Recovery::Verified,
        }),
        Ok(snapshot(Evidence::TransportAccepted)),
        Ok(lighting::Snapshot {
            revision: vec![99],
            ..snapshot(Evidence::Readback)
        }),
        Ok(lighting::Snapshot {
            picture_context: vec![9],
            ..snapshot(Evidence::Readback)
        }),
    ] {
        let mut session = loaded_host();
        let start = session.start_host("screen", None).unwrap();
        session.accept_host(event(start.ticket, HostEventKind::Started));
        assert!(matches!(
            session.accept_host(finished(start.ticket, restoration, None)),
            HostOutcome::Failed(_)
        ));
        assert!(session.requires_manual_read());
        assert!(matches!(
            session.lighting().unwrap().problem(),
            Some(Problem::Apply(_))
        ));
        assert_eq!(
            session.lighting().unwrap().baseline(),
            Some(&snapshot(Evidence::Readback))
        );
        assert_eq!(
            session.lighting().unwrap().draft(),
            Some(&setting("steady", 2))
        );
    }
}
#[test]
fn disconnect_clears_host_with_unverified_hold_and_stale_events_cannot_settle_a_new_activity() {
    let mut session = loaded_host();
    let first = session.start_host("screen", None).unwrap();
    session.accept_host(event(first.ticket, HostEventKind::Started));
    session.disconnect().unwrap();
    assert!(session.host().is_idle());
    assert!(matches!(
        session.lighting().unwrap().problem(),
        Some(Problem::Apply(ApplyFailure {
            recovery: Recovery::Unverified,
            ..
        }))
    ));
    session.connect().unwrap();
    assert!(session.start_host("screen", None).is_err());
    let read = session.read_lighting().unwrap();
    session.accept(Completion {
        generation: read.generation,
        operation: read.operation,
        payload: CompletionPayload::Lighting(FeatureResult::Read(Ok(snapshot(Evidence::Readback)))),
    });
    let second = session.start_host("screen", None).unwrap();
    assert_ne!(first.ticket, second.ticket);
    assert_eq!(
        session.accept_host(finished(
            first.ticket,
            Ok(snapshot(Evidence::Readback)),
            None
        )),
        HostOutcome::Ignored
    );
    assert_eq!(session.host().ticket(), Some(second.ticket));
    assert_eq!(session.host().phase(), Phase::Starting);
}

#[test]
fn live_parameters_require_active_valid_mode_and_retain_original_baseline() {
    let mut session = loaded_host();
    assert!(session.update_host(setting("audio", 3)).is_err());
    let start = session.start_host("audio", None).unwrap();
    assert!(session.update_host(setting("audio", 3)).is_err());
    session.accept_host(HostEvent {
        ticket: start.ticket,
        kind: HostEventKind::Started,
    });
    let update = session.update_host(setting("audio", 3)).unwrap();
    assert_eq!(update.ticket, start.ticket);
    assert_eq!(update.setting, setting("audio", 3));
    assert!(session.update_host(setting("audio", 4)).is_err());
    assert!(session.update_host(setting("steady", 3)).is_err());
    assert_eq!(
        session.lighting().unwrap().baseline(),
        Some(&start.expected)
    );
    assert_eq!(session.host().phase(), Phase::Active);
    session.stop_host();
    assert!(session.update_host(setting("audio", 3)).is_err());
}
