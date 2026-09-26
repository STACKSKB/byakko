use crate::model::archive::*;
use std::collections::BTreeSet;
pub fn validate_capabilities(caps: &ArchiveCapabilities) -> Result<(), String> {
    if caps.backend_id.is_empty() || caps.format_id.is_empty() || caps.max_bytes == 0 {
        return Err("Invalid native archive capabilities".into());
    }
    Ok(())
}

pub fn validate_archive(caps: &ArchiveCapabilities, archive: &NativeArchive) -> Result<(), String> {
    if archive.backend_id != caps.backend_id || archive.format_id != caps.format_id {
        return Err("Native archive belongs to a different backend or format".into());
    }
    if archive.bytes.is_empty() || archive.bytes.len() > caps.max_bytes as usize {
        return Err("Native archive is empty or exceeds the advertised size limit".into());
    }
    Ok(())
}

pub fn validate_review(caps: &ArchiveCapabilities, review: &Review) -> Result<(), String> {
    validate_archive(caps, &review.before)?;
    validate_archive(caps, &review.target)?;
    let mut seen = BTreeSet::new();
    for section in &review.changes {
        if section.id.is_empty() || section.label.is_empty() || !seen.insert(&section.id) {
            return Err("Invalid or duplicate native archive section".into());
        }
    }
    Ok(())
}
