use byakko_devices::nia87::device;

/// Select one configuration collection once, then retain its exact identity.
pub fn target() -> device::Result<device::Target> {
    let candidates = device::candidates()?;
    let [candidate] = candidates.as_slice() else {
        return Err(format!(
            "Expected one Nia87 configuration collection; found {}",
            candidates.len()
        )
        .into());
    };
    Ok(device::Target::from_candidate(candidate)?)
}

#[allow(dead_code)] // Each example compiles this shared module independently.
pub fn access() -> device::Result<device::Access> {
    Ok(device::Access::bound(target()?))
}

#[allow(dead_code)] // Only archive research examples need the typed recovery conversion.
pub fn apply_configuration(
    access: &device::Access,
    current: &byakko_protocol::nia87::configuration::Configuration,
    desired: &byakko_protocol::nia87::configuration::Configuration,
    backup_dir: &std::path::Path,
    progress: impl FnMut(&str),
) -> device::Result<byakko_protocol::nia87::configuration::Configuration> {
    access
        .apply_configuration(current, desired, backup_dir, progress)
        .map_err(|failure| format!("{}; recovery: {:?}", failure.message, failure.recovery).into())
}
