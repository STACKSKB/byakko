//! Pure Nia87 diagnostic archive projection for core and browser clients.

use byakko_core::model::archive::{ArchiveCapabilities, NativeArchive};

use super::configuration::{self, Configuration};

const BACKEND_ID: &str = "nia87";

pub fn capabilities() -> ArchiveCapabilities {
    ArchiveCapabilities {
        backend_id: BACKEND_ID.into(),
        format_id: configuration::ARCHIVE_FORMAT_ID.into(),
        max_bytes: configuration::MAX_ARCHIVE_BYTES,
    }
}

pub fn encode(config: &Configuration) -> Result<NativeArchive, String> {
    let bytes = configuration::encode(config).map_err(|error| error.to_string())?;
    Ok(NativeArchive {
        backend_id: BACKEND_ID.into(),
        format_id: configuration::ARCHIVE_FORMAT_ID.into(),
        bytes,
    })
}

pub fn decode(archive: &NativeArchive) -> Result<Configuration, String> {
    if archive.backend_id != BACKEND_ID || archive.format_id != configuration::ARCHIVE_FORMAT_ID {
        return Err("Archive belongs to another backend or format".into());
    }
    if archive.bytes.len() > configuration::MAX_ARCHIVE_BYTES as usize {
        return Err("Configuration archive is too large".into());
    }
    configuration::decode(&archive.bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_wrapper_round_trips_shared_diagnostic_archive() {
        let raw = configuration::tests::example();
        let archive = encode(&raw).unwrap();
        assert_eq!(archive.backend_id, "nia87");
        assert_eq!(archive.format_id, "byakko-configuration-v1");
        assert_eq!(decode(&archive).unwrap(), raw);
    }
}
