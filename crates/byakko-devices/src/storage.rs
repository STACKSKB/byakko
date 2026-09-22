//! Stable per-user GUI data location. No current-directory fallback.
use std::{ffi::OsString, io, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Windows,
    Linux,
}

fn absolute_for(platform: Platform, value: &str) -> bool {
    match platform {
        Platform::Windows => {
            let bytes = value.as_bytes();
            let drive_absolute = bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && (bytes[2] == b'\\' || bytes[2] == b'/');
            let unc_absolute = (value.starts_with("\\\\") || value.starts_with("//"))
                && value[2..]
                    .split(['\\', '/'])
                    .take(2)
                    .all(|part| !part.is_empty())
                && value[2..].split(['\\', '/']).count() >= 2;
            drive_absolute || unc_absolute
        }
        Platform::Linux => value.starts_with('/'),
    }
}

/// Resolve from injected environment values so platform rules can be tested
/// without mutating the process environment.
pub fn resolve_with(
    platform: Platform,
    get: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    let candidates: &[(&str, &[&str])] = match platform {
        Platform::Windows => &[
            ("LOCALAPPDATA", &["Byakko"]),
            ("USERPROFILE", &["AppData", "Local", "Byakko"]),
        ],
        Platform::Linux => &[
            ("XDG_DATA_HOME", &["byakko"]),
            ("HOME", &[".local", "share", "byakko"]),
        ],
    };
    for (variable, suffix) in candidates {
        let Some(value) = get(variable) else { continue };
        let Some(text) = value.to_str() else { continue };
        if !absolute_for(platform, text) {
            continue;
        }
        let mut path = PathBuf::from(value);
        for component in *suffix {
            path.push(component);
        }
        return Ok(path);
    }
    let candidates = match platform {
        Platform::Windows => "LOCALAPPDATA or USERPROFILE",
        Platform::Linux => "XDG_DATA_HOME or HOME",
    };
    Err(format!(
        "No absolute per-user Byakko data directory is available; set {candidates} to an absolute path"
    ))
}

pub fn user_data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let platform = Platform::Windows;
    #[cfg(target_os = "linux")]
    let platform = Platform::Linux;
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return Err("No supported per-user Byakko data directory on this platform".into());
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    resolve_with(platform, |name| std::env::var_os(name))
}

pub fn prepare_user_data_dir() -> io::Result<PathBuf> {
    let path =
        user_data_dir().map_err(|message| io::Error::new(io::ErrorKind::NotFound, message))?;
    std::fs::create_dir_all(&path)?;
    std::fs::create_dir_all(path.join("backups"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn env<'a>(values: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| OsString::from(value))
        }
    }
    #[test]
    fn windows_prefers_local_appdata_and_falls_back_to_profile() {
        let primary = resolve_with(
            Platform::Windows,
            env(&[
                ("LOCALAPPDATA", "C:\\Users\\A\\AppData\\Local"),
                ("USERPROFILE", "C:\\Users\\A"),
            ]),
        )
        .unwrap();
        assert_eq!(
            primary.to_string_lossy().replace('/', "\\"),
            "C:\\Users\\A\\AppData\\Local\\Byakko"
        );
        let fallback = resolve_with(
            Platform::Windows,
            env(&[
                ("LOCALAPPDATA", "relative"),
                ("USERPROFILE", "C:\\Users\\A"),
            ]),
        )
        .unwrap();
        assert_eq!(
            fallback.to_string_lossy().replace('/', "\\"),
            "C:\\Users\\A\\AppData\\Local\\Byakko"
        );
        let unc = resolve_with(
            Platform::Windows,
            env(&[("LOCALAPPDATA", "\\\\server\\share\\users\\A")]),
        )
        .unwrap();
        assert_eq!(
            unc.to_string_lossy().replace('/', "\\"),
            "\\\\server\\share\\users\\A\\Byakko"
        );
        assert!(resolve_with(Platform::Windows, env(&[("LOCALAPPDATA", "relative")])).is_err());
        assert!(resolve_with(Platform::Windows, env(&[("LOCALAPPDATA", "C:relative")])).is_err());
        assert!(resolve_with(Platform::Windows, env(&[("LOCALAPPDATA", "\\rooted")])).is_err());
    }
    #[test]
    fn linux_prefers_xdg_and_falls_back_to_home() {
        let primary = resolve_with(
            Platform::Linux,
            env(&[("XDG_DATA_HOME", "/data/me"), ("HOME", "/home/me")]),
        )
        .unwrap();
        assert_eq!(
            primary.to_string_lossy().replace('\\', "/"),
            "/data/me/byakko"
        );
        let fallback = resolve_with(
            Platform::Linux,
            env(&[("XDG_DATA_HOME", "relative"), ("HOME", "/home/me")]),
        )
        .unwrap();
        assert_eq!(
            fallback.to_string_lossy().replace('\\', "/"),
            "/home/me/.local/share/byakko"
        );
        assert!(resolve_with(Platform::Linux, env(&[("HOME", "relative")])).is_err());
    }
}
