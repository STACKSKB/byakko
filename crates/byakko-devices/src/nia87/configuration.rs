//! Native file I/O for the shared lossless Nia87 archive format.

use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use byakko_protocol::nia87::configuration::{self, Configuration, MAX_ARCHIVE_BYTES};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Create a new archive exclusively and flush it before reporting success.
pub fn save_new(path: &Path, config: &Configuration) -> Result<()> {
    let bytes = configuration::encode(config)?;
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    Ok(())
}

/// Read no more than the maximum archive size plus one byte from a local file.
pub fn load(path: &Path) -> Result<Configuration> {
    let input = File::open(path)?;
    let mut bytes = Vec::new();
    input
        .take(u64::from(MAX_ARCHIVE_BYTES) + 1)
        .read_to_end(&mut bytes)?;
    configuration::decode(&bytes)
}

#[cfg(test)]
pub(crate) mod tests {
    use byakko_protocol::nia87::{
        adapter::Snapshot, configuration::Configuration, lighting::Lighting, settings::Settings,
    };

    pub(crate) fn example() -> Configuration {
        let mut settings = [[0u8; 64]; 4];
        for (reply, opcode) in settings.iter_mut().zip([0x91, 0x97, 0x92, 0x86]) {
            reply[0] = opcode;
        }
        let mut lighting = [0; 64];
        lighting[0] = 0x87;
        Configuration {
            keymaps: Snapshot {
                format_version: 1,
                firmware: 0x0100,
                profile: 0,
                base: vec![[0; 4]; 128],
                function: vec![[0; 4]; 128],
            },
            macros: vec![vec![0; 256]; 50],
            lighting: Lighting::decode(&lighting).unwrap(),
            picture: vec![[0; 3]; 128],
            settings: Settings::decode(&settings[0], &settings[1], &settings[2], &settings[3])
                .unwrap(),
        }
    }
}
