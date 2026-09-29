//! Native Nia87 picture effects.
use crate::nia87::device;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::picture,
};
use byakko_protocol::nia87::picture_adapter::{desired_native, project};
use std::{collections::BTreeMap, path::Path};

pub(super) fn read_with(access: &device::Access) -> Result<picture::Snapshot, String> {
    let (colors, context) = access
        .read_picture_with_context()
        .map_err(|error| error.to_string())?;
    project(&colors, context)
}

pub(super) fn apply_with(
    access: &device::Access,
    expected: &picture::Snapshot,
    desired: &BTreeMap<String, [u8; 3]>,
    backup: &Path,
) -> Result<picture::Snapshot, ApplyFailure> {
    let (original, target, context) = desired_native(expected, desired).map_err(not_attempted)?;
    let actual = access.apply_picture(&original, &target, context, backup)?;
    project(&actual, context)
        .map(|mut snapshot| {
            snapshot.evidence = if target == original {
                expected.evidence
            } else {
                picture::Evidence::TransportAccepted
            };
            snapshot
        })
        .map_err(|message| ApplyFailure {
            message,
            recovery: Recovery::Unverified,
        })
}

fn not_attempted(message: String) -> ApplyFailure {
    ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    }
}
