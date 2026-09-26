//! Single-pass bounded reads and exclusive durable local exports.
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub fn read_json<T: DeserializeOwned>(reader: impl Read, limit: u64) -> Result<T, String> {
    let bytes = read_bounded(reader, limit)?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}
fn read_bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit.checked_add(1).ok_or("Invalid file limit")?)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("File exceeds its size limit".into());
    }
    Ok(bytes)
}
pub fn read_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Path is not a regular file".into());
    }
    read_bounded(file, limit)
}
pub fn load_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T, String> {
    serde_json::from_slice(&read_bytes(path, limit)?).map_err(|error| error.to_string())
}
/// Reserve an export before device discovery. An existing file is never opened for writing.
pub fn reserve_new(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())
}
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_bytes(reserve_new(path)?, bytes)
}
fn write_bytes(mut file: File, bytes: &[u8]) -> Result<(), String> {
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())
}
pub fn write_json(file: File, value: &impl Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    write_bytes(file, &bytes)
}
pub fn save_new_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    write_json(reserve_new(path)?, value)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "byakko-json-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[test]
    fn exact_limit_and_trailing_content_have_one_parser_boundary() {
        assert_eq!(read_json::<String>(b"\"ok\"".as_slice(), 4).unwrap(), "ok");
        assert!(read_json::<String>(b"\"ok\" ".as_slice(), 4).is_err());
        assert!(read_json::<String>(b"\"ok\" {}".as_slice(), 64).is_err());
    }
    #[test]
    fn durable_export_does_not_overwrite_existing_file() {
        let path = path();
        save_new_json(&path, &vec![1, 2, 3]).unwrap();
        assert_eq!(load_json::<Vec<u8>>(&path, 64).unwrap(), vec![1, 2, 3]);
        assert!(save_new_json(&path, &vec![4]).is_err());
        assert_eq!(load_json::<Vec<u8>>(&path, 64).unwrap(), vec![1, 2, 3]);
        std::fs::remove_file(path).unwrap();
    }
}
