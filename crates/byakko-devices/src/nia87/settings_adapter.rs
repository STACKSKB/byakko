//! Native effect delivery for the shared Nia87 settings projection and plan.

use super::device;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::settings::{Content, Edit, Snapshot},
};
use byakko_protocol::nia87::settings_adapter;
use std::path::Path;

pub(super) fn read_with(access: &device::Access) -> Result<Snapshot, String> {
    let settings = access.read_settings().map_err(|error| error.to_string())?;
    Ok(settings_adapter::project(&settings))
}

pub(super) fn apply_with(
    access: &device::Access,
    expected: &Snapshot,
    edit: &Edit,
    backup: &Path,
) -> Result<Snapshot, ApplyFailure> {
    let (expected_native, setting) =
        settings_adapter::validated_edit(expected, edit).map_err(|message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        })?;
    let actual = access.apply_setting(&expected_native, setting, backup)?;
    let snapshot = settings_adapter::project(&actual);
    if !matches!(&snapshot.content, Content::Editable(values) if values.get(&edit.id) == Some(&edit.value))
    {
        return Err(ApplyFailure {
            message: "Settings readback does not match the requested field".into(),
            recovery: Recovery::Unverified,
        });
    }
    Ok(snapshot)
}
