//! Native Nia87 lighting effects.
use crate::nia87::device;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::lighting::{self, Content, Setting, Snapshot},
};
use byakko_protocol::nia87::{
    lighting as native,
    lighting_adapter::{draft, from_native},
};
use std::path::Path;

pub(super) fn read_with(access: &device::Access) -> Result<Snapshot, String> {
    let raw = access.read_lighting().map_err(|error| error.to_string())?;
    Ok(from_native(&raw))
}

pub(super) fn apply_with(
    access: &device::Access,
    expected: &Snapshot,
    desired: &Setting,
    backup: &Path,
) -> Result<Snapshot, ApplyFailure> {
    let native_setting = draft(expected, desired).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let expected_native =
        native::Lighting::decode(&expected.revision).map_err(|message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        })?;
    let actual = access.apply_lighting(&expected_native, &native_setting, backup)?;
    let mut snapshot = from_native(&actual);
    if snapshot.revision != expected.revision {
        snapshot.evidence = lighting::Evidence::TransportAccepted;
    } else {
        snapshot.evidence = expected.evidence;
    }
    if snapshot.content != Content::Editable(desired.clone()) {
        return Err(ApplyFailure {
            message: "Submitted lighting cannot be represented as requested".into(),
            recovery: Recovery::Unverified,
        });
    }
    Ok(snapshot)
}
