// SPDX-License-Identifier: GPL-3.0-or-later
use super::{Metadata, Speed, Unavailable, interrupt_endpoints};
use crate::tester_input::InputDevice;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};
const DESCRIPTOR_LIMIT: u64 = 1_048_576;

pub fn inspect(device: &InputDevice) -> Result<Metadata, Unavailable> {
    let path = Path::new(&device.id);
    let event = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| {
            name.strip_prefix("event").is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
        .ok_or_else(|| Unavailable("Selected source is not an evdev input device".into()))?;
    if path.parent() != Some(Path::new("/dev/input")) {
        return Err(Unavailable(
            "Selected source is not an evdev input device".into(),
        ));
    }
    inspect_sysfs(&Path::new("/sys/class/input").join(event), &device.identity)
}
fn inspect_sysfs(input: &Path, identity: &str) -> Result<Metadata, Unavailable> {
    let unavailable = |error: std::io::Error| {
        Unavailable(format!("USB descriptor metadata unavailable: {error}"))
    };
    let selected = fs::canonicalize(input).map_err(unavailable)?;
    if selected.to_string_lossy() != identity {
        return Err(Unavailable("Selected input device identity changed".into()));
    }
    let interface_path = selected
        .ancestors()
        .take(32)
        .find(|path| path.join("bInterfaceNumber").is_file())
        .ok_or_else(|| {
            Unavailable("Selected input has no inspectable USB interface ancestor".into())
        })?;
    let interface = hex_attribute(interface_path, "bInterfaceNumber")?;
    let alternate = decimal_attribute(interface_path, "bAlternateSetting")?;
    if hex_attribute(interface_path, "bInterfaceClass")? != 3 {
        return Err(Unavailable(
            "Selected input USB interface is not HID".into(),
        ));
    }
    let usb_path = interface_path
        .ancestors()
        .skip(1)
        .take(32)
        .find(|path| path.join("descriptors").is_file() && path.join("speed").is_file())
        .ok_or_else(|| {
            Unavailable("Selected USB interface has no device descriptor ancestor".into())
        })?;
    let speed = match attribute(usb_path, "speed")?.as_str() {
        "1.5" => Speed::Low,
        "12" => Speed::Full,
        "480" => Speed::High,
        "5000" | "10000" | "20000" => Speed::Super,
        _ => {
            return Err(Unavailable(
                "Selected USB device speed is unavailable".into(),
            ));
        }
    };
    let configuration = attribute(usb_path, "bConfigurationValue")?
        .parse::<u8>()
        .map_err(|_| Unavailable("Invalid active USB configuration".into()))?;
    let mut bytes = Vec::new();
    File::open(usb_path.join("descriptors"))
        .map_err(unavailable)?
        .take(DESCRIPTOR_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(unavailable)?;
    if bytes.len() as u64 > DESCRIPTOR_LIMIT {
        return Err(Unavailable(
            "USB descriptor data exceeds inspection limit".into(),
        ));
    }
    let endpoints = interrupt_endpoints(&bytes, configuration, interface, alternate, speed)?;
    // This exact input sysfs node must still identify the selected event device.
    if fs::canonicalize(input).map_err(unavailable)? != selected
        || decimal_attribute(interface_path, "bAlternateSetting")? != alternate
        || attribute(usb_path, "bConfigurationValue")?
            .parse::<u8>()
            .ok()
            != Some(configuration)
    {
        return Err(Unavailable(
            "Selected input or active USB setting changed during inspection".into(),
        ));
    }
    Ok(Metadata {
        source: format!("Linux sysfs {}", usb_path.display()),
        speed,
        endpoints,
    })
}
fn attribute(path: &Path, name: &str) -> Result<String, Unavailable> {
    let mut value = String::new();
    File::open(path.join(name))
        .and_then(|file| file.take(65).read_to_string(&mut value))
        .map_err(|error| Unavailable(format!("USB {name} unavailable: {error}")))?;
    if value.len() > 64 {
        return Err(Unavailable(format!("USB {name} exceeds inspection limit")));
    }
    Ok(value.trim().to_owned())
}
fn hex_attribute(path: &Path, name: &str) -> Result<u8, Unavailable> {
    u8::from_str_radix(&attribute(path, name)?, 16)
        .map_err(|_| Unavailable(format!("Invalid USB {name}")))
}

fn decimal_attribute(path: &Path, name: &str) -> Result<u8, Unavailable> {
    attribute(path, name)?
        .parse::<u8>()
        .map_err(|_| Unavailable(format!("Invalid USB {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("byakko-usb-fixture-{}-{stamp}", std::process::id()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
            assert!(
                self.0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("byakko-usb-fixture-")
            );
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn sysfs_uses_exact_input_ancestor_and_decimal_alternate_setting() {
        let fixture = Fixture::new();
        let usb = fixture.0.join("usb-device");
        let iface = usb.join("interface");
        let event = iface.join("input/input9/event12");
        fs::create_dir_all(&event).unwrap();
        for (path, value) in [
            (iface.join("bInterfaceNumber"), "0a\n"),
            (iface.join("bAlternateSetting"), "10\n"),
            (iface.join("bInterfaceClass"), "03\n"),
            (usb.join("speed"), "480\n"),
            (usb.join("bConfigurationValue"), "2\n"),
        ] {
            fs::write(path, value).unwrap();
        }
        let descriptors = [
            9, 2, 25, 0, 1, 2, 0, 0x80, 50, 9, 4, 10, 10, 1, 3, 1, 1, 0, 7, 5, 0x81, 3, 8, 0, 4,
        ];
        fs::write(usb.join("descriptors"), descriptors).unwrap();
        let alias = fixture.0.join("event12");
        std::os::unix::fs::symlink(&event, &alias).unwrap();
        let identity = fs::canonicalize(&alias)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let metadata = inspect_sysfs(&alias, &identity).unwrap();
        assert_eq!(metadata.speed, Speed::High);
        assert_eq!(metadata.endpoints[0].interface, 10);
        assert_eq!(metadata.endpoints[0].alternate, 10);
        assert_eq!(metadata.endpoints[0].interval_us, 1000);
        assert!(inspect_sysfs(&alias, "different-input-identity").is_err());
        fs::write(usb.join("speed"), "unknown\n").unwrap();
        assert!(inspect_sysfs(&alias, &identity).is_err());
    }
}
