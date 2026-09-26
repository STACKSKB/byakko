use super::*;
use crate::{
    library::archive::{CaptureProblem, CaptureStatus},
    model::archive::{ArchiveCapabilities, NativeArchive},
};
fn configured() -> Session {
    loaded()
        .with_archive(ArchiveCapabilities {
            backend_id: "test".into(),
            format_id: "native".into(),
            max_bytes: 4,
        })
        .unwrap()
}
fn archive(bytes: &[u8]) -> NativeArchive {
    NativeArchive {
        backend_id: "test".into(),
        format_id: "native".into(),
        bytes: bytes.into(),
    }
}
fn completion(command: &Command, result: FeatureResult<NativeArchive>) -> Completion {
    Completion {
        generation: command.generation,
        operation: command.operation,
        payload: CompletionPayload::Archive(result),
    }
}
#[test]
fn archive_capture_is_correlated_read_only_and_preserves_editable_caches() {
    let mut s = configured();
    edit(&mut s);
    let baseline = s.keymap().baseline().cloned();
    let draft = s.keymap().draft().cloned();
    let command = s.capture_archive().unwrap();
    assert!(matches!(
        command.payload,
        CommandPayload::Archive(FeatureCommand::Read(()))
    ));
    assert!(s.read().is_err());
    let mut stale = completion(&command, FeatureResult::Read(Ok(archive(&[1, 2]))));
    stale.generation += 1;
    assert_eq!(s.accept(stale), Outcome::Ignored);
    assert_eq!(
        s.accept(completion(
            &command,
            FeatureResult::Apply(Ok(archive(&[1, 2])))
        )),
        Outcome::Ignored
    );
    assert!(s.busy());
    assert_eq!(
        s.accept(completion(
            &command,
            FeatureResult::Read(Ok(archive(&[1, 2])))
        )),
        Outcome::ArchiveCaptured
    );
    assert_eq!(s.archive().unwrap().status(), &CaptureStatus::Ready);
    assert_eq!(s.archive().unwrap().captured(), Some(&archive(&[1, 2])));
    assert_eq!(s.keymap().baseline(), baseline.as_ref());
    assert_eq!(s.keymap().draft(), draft.as_ref());
    assert_eq!(s.keymap().status(), &Status::Ready);
    assert!(s.keymap().dirty());
    assert!(!s.busy());
}
#[test]
fn failed_or_invalid_capture_retains_last_success_without_claiming_latest_success() {
    let mut s = configured();
    let command = s.capture_archive().unwrap();
    s.accept(completion(&command, FeatureResult::Read(Ok(archive(&[1])))));
    let command = s.capture_archive().unwrap();
    let problem = CaptureProblem::Capture("failed".into());
    assert_eq!(
        s.accept(completion(
            &command,
            FeatureResult::Read(Err("failed".into()))
        )),
        Outcome::ArchiveCaptureFailed(problem.clone())
    );
    assert_eq!(
        s.archive().unwrap().status(),
        &CaptureStatus::Failed(problem)
    );
    assert_eq!(s.archive().unwrap().captured(), Some(&archive(&[1])));
    for invalid in [
        NativeArchive {
            backend_id: "other".into(),
            ..archive(&[1])
        },
        NativeArchive {
            format_id: "other".into(),
            ..archive(&[1])
        },
        archive(&[]),
        archive(&[1, 2, 3, 4, 5]),
    ] {
        let command = s.capture_archive().unwrap();
        assert!(matches!(
            s.accept(completion(&command, FeatureResult::Read(Ok(invalid)))),
            Outcome::ArchiveCaptureFailed(CaptureProblem::InvalidResult(_))
        ));
        assert_eq!(s.archive().unwrap().captured(), Some(&archive(&[1])));
    }
    let command = s.capture_archive().unwrap();
    assert_eq!(
        s.accept(completion(&command, FeatureResult::Read(Ok(archive(&[2]))))),
        Outcome::ArchiveCaptured
    );
    assert_eq!(s.archive().unwrap().captured(), Some(&archive(&[2])));
}
#[test]
fn disconnect_marks_pending_capture_failed_and_rejects_old_result() {
    let mut s = configured();
    let command = s.capture_archive().unwrap();
    s.accept(completion(&command, FeatureResult::Read(Ok(archive(&[1])))));
    let command = s.capture_archive().unwrap();
    s.disconnect().unwrap();
    assert!(matches!(
        s.archive().unwrap().status(),
        CaptureStatus::Failed(CaptureProblem::Capture(_))
    ));
    assert_eq!(s.archive().unwrap().captured(), Some(&archive(&[1])));
    s.connect().unwrap();
    assert_eq!(
        s.accept(completion(&command, FeatureResult::Read(Ok(archive(&[2]))))),
        Outcome::Ignored
    );
    assert_eq!(s.archive().unwrap().captured(), Some(&archive(&[1])));
}
