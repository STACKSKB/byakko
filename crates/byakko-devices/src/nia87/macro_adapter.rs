//! Native Nia87 macro effects.
use crate::nia87::device;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::macros::{Program, Snapshot},
};
use byakko_protocol::nia87::macro_adapter::{from_bytes, prepare, slot_number};
use std::path::Path;

pub(super) fn read_with(access: &device::Access, slot: &str) -> Result<Snapshot, String> {
    let number = slot_number(slot)?;
    let raw = access
        .read_macro(number)
        .map_err(|error| error.to_string())?;
    from_bytes(slot, &raw)
}

pub(super) fn read_catalog_with(
    access: &device::Access,
    slots: &[String],
) -> Result<Vec<Snapshot>, String> {
    let numbers = slots
        .iter()
        .map(|slot| slot_number(slot))
        .collect::<Result<Vec<_>, _>>()?;
    let raw = access
        .read_macros(&numbers)
        .map_err(|error| error.to_string())?;
    slots
        .iter()
        .zip(raw)
        .map(|(slot, bytes)| from_bytes(slot, &bytes))
        .collect()
}

pub(super) fn apply_with(
    access: &device::Access,
    expected: &Snapshot,
    desired: &Program,
    backup: &Path,
) -> Result<Snapshot, ApplyFailure> {
    let (before, value) = prepare(expected, desired).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let number = slot_number(&expected.slot).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let raw = access.apply_macro_validated(number, &before, &value, backup)?;
    from_bytes(&expected.slot, &raw).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::Unverified,
    })
}
