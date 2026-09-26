use crate::model::{keymap::Descriptor, picture::*};
use std::collections::BTreeSet;
pub fn validate_capabilities(caps: &Capabilities, descriptor: &Descriptor) -> Result<(), String> {
    if caps.backend_id.is_empty()
        || caps.backend_id != descriptor.backend_id
        || caps.keys.is_empty()
        || caps
            .lighting_effect
            .as_ref()
            .is_some_and(|id| id.trim().is_empty())
    {
        return Err("Invalid picture backend or empty key catalog".into());
    }
    let mut seen = BTreeSet::new();
    for id in &caps.keys {
        if id.is_empty() || !seen.insert(id) {
            return Err("Duplicate or empty picture key ID".into());
        }
        if !descriptor.keys.iter().any(|key| key.id == *id) {
            return Err(format!("Unknown picture key: {id}"));
        }
    }
    Ok(())
}

pub fn validate_snapshot(caps: &Capabilities, snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.backend_id != caps.backend_id {
        return Err("Picture result belongs to a different backend".into());
    }
    if let Content::Editable(colors) = &snapshot.content {
        let keys: BTreeSet<_> = caps.keys.iter().collect();
        if colors.len() != keys.len() || colors.keys().any(|id| !keys.contains(id)) {
            return Err("Picture has missing or unknown keys".into());
        }
    }
    Ok(())
}
