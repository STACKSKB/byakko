//! Linux hidraw enumeration and Nia87 configuration transport.

use std::{
    ffi::{CStr, CString, OsStr},
    fs::{self, File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::ffi::OsStrExt},
    path::{Path, PathBuf},
};

use super::{DeviceInfo, Result, descriptor::parse_descriptor};

const MAX_DESCRIPTOR: usize = 4096;
const HOST_REPORT_LEN: usize = 65;
const HIDRAW_CLASS: &str = "/sys/class/hidraw";

mod input;
pub(crate) use input::InputDevice;

const fn ioc(direction: u32, number: u32, size: u32) -> libc::c_ulong {
    ((direction << 30) | (size << 16) | ((b'H' as u32) << 8) | number) as libc::c_ulong
}
const IOC_READ: u32 = 2;
const IOC_WRITE: u32 = 1;
pub(super) const HIDIOCGRDESCSIZE: libc::c_ulong = ioc(IOC_READ, 0x01, 4);
pub(super) const HIDIOCGRDESC: libc::c_ulong = ioc(IOC_READ, 0x02, 4 + MAX_DESCRIPTOR as u32);
pub(super) const HIDIOCGRAWINFO: libc::c_ulong = ioc(IOC_READ, 0x03, 8);

#[repr(C)]
struct ReportDescriptor {
    size: u32,
    value: [u8; MAX_DESCRIPTOR],
}

#[repr(C)]
#[derive(Default)]
pub(super) struct RawInfo {
    pub(super) bus_type: u32,
    pub(super) vendor: i16,
    pub(super) product: i16,
}

pub struct Device {
    file: File,
}

impl Device {
    pub fn send_feature_report(&self, report: &[u8]) -> Result<()> {
        if report.len() != HOST_REPORT_LEN || report[0] != 0 {
            return Err("Nia87 feature report must be 65 bytes with report ID zero".into());
        }
        let mut buffer = report.to_vec();
        let request = ioc(IOC_READ | IOC_WRITE, 0x06, buffer.len() as u32);
        let status = unsafe { libc::ioctl(self.file.as_raw_fd(), request, buffer.as_mut_ptr()) };
        if status < 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }

    pub fn get_feature_report(&self, report: &mut [u8]) -> Result<usize> {
        if report.len() != HOST_REPORT_LEN || report[0] != 0 {
            return Err("Nia87 feature report buffer must be 65 bytes with report ID zero".into());
        }
        let mut buffer = vec![0u8; report.len()];
        buffer[0] = report[0];
        let request = ioc(IOC_READ | IOC_WRITE, 0x07, buffer.len() as u32);
        let status = unsafe { libc::ioctl(self.file.as_raw_fd(), request, buffer.as_mut_ptr()) };
        if status < 0 {
            return Err(io::Error::last_os_error().into());
        }
        normalize_unnumbered_feature_reply(report, &buffer, status as usize)
    }

    pub fn get_report_descriptor(&self, output: &mut [u8]) -> Result<usize> {
        let descriptor = descriptor_from_fd(self.file.as_raw_fd())?;
        if output.len() < descriptor.len() {
            return Err(format!(
                "descriptor needs {} bytes, buffer has {}",
                descriptor.len(),
                output.len()
            )
            .into());
        }
        output[..descriptor.len()].copy_from_slice(&descriptor);
        Ok(descriptor.len())
    }
}

fn normalize_unnumbered_feature_reply(
    report: &mut [u8],
    buffer: &[u8],
    count: usize,
) -> Result<usize> {
    if report.len() != HOST_REPORT_LEN || buffer.len() != HOST_REPORT_LEN || count > buffer.len() {
        return Err("hidraw returned an invalid feature reply length".into());
    }
    match count {
        count if count == HOST_REPORT_LEN - 1 => {
            report[0] = 0;
            report[1..].copy_from_slice(&buffer[..count]);
        }
        HOST_REPORT_LEN if buffer[0] == 0 => report.copy_from_slice(buffer),
        _ => return Err("hidraw returned an incompatible unnumbered feature reply".into()),
    }
    Ok(HOST_REPORT_LEN)
}

pub fn open(path: &CStr) -> Result<Device> {
    let path = Path::new(OsStr::from_bytes(path.to_bytes()));
    if path.parent() != Some(Path::new("/dev"))
        || !path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(valid_hidraw_name)
    {
        return Err("path must be a /dev/hidrawN node".into());
    }
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let mut info = RawInfo::default();
    if unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCGRAWINFO, &mut info) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    if info.vendor as u16 != 0x3151 || !matches!(info.product as u16, 0x4011 | 0x4015) {
        return Err("hidraw node is not a supported Nia87 VID/PID".into());
    }
    let descriptor = descriptor_from_fd(file.as_raw_fd())?;
    let parsed = parse_descriptor(&descriptor).ok_or("Malformed HID report descriptor")?;
    if !parsed.target {
        return Err("hidraw node lacks the Nia87 vendor application collection".into());
    }
    if parsed.target_feature_count != 1 || !parsed.target_feature_shape_valid {
        return Err("Nia87 collection must have one unnumbered 64-byte feature report".into());
    }
    Ok(Device { file })
}

pub fn enumerate() -> Result<Vec<DeviceInfo>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(HIDRAW_CLASS)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !valid_hidraw_name(name_str) {
            continue;
        }
        let sys_device = entry.path().join("device");
        let Some((vid, pid)) = fs::read_to_string(sys_device.join("uevent"))
            .ok()
            .and_then(|text| hid_id(&text))
        else {
            continue;
        };
        let parsed = fs::read(sys_device.join("report_descriptor"))
            .ok()
            .filter(|descriptor| !descriptor.is_empty() && descriptor.len() <= MAX_DESCRIPTOR)
            .and_then(|descriptor| parse_descriptor(&descriptor));
        let canonical = fs::canonicalize(&sys_device).unwrap_or_else(|_| sys_device.clone());
        let interface = ancestor_hex(&canonical, "bInterfaceNumber").map_or(-1, i32::from);
        let release = ancestor_hex(&canonical, "bcdDevice").unwrap_or(0);
        let manufacturer = ancestor_string(&canonical, "manufacturer");
        let product = ancestor_string(&canonical, "product").or_else(|| {
            fs::read_to_string(sys_device.join("uevent"))
                .ok()
                .and_then(|text| {
                    text.lines()
                        .find_map(|line| line.strip_prefix("HID_NAME=").map(str::to_owned))
                })
        });
        let dev_path = PathBuf::from("/dev").join(name);
        found.push(DeviceInfo {
            physical_device: usb_device(&canonical).map(|path| path.to_string_lossy().into_owned()),
            path: CString::new(dev_path.as_os_str().as_bytes())?,
            vid,
            pid,
            interface,
            usage_page: parsed
                .and_then(|value| value.application_usage)
                .map_or(0, |value| value.0),
            usage: parsed
                .and_then(|value| value.application_usage)
                .map_or(0, |value| value.1),
            manufacturer,
            product,
            release,
        });
    }
    found.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(found)
}

fn usb_device(path: &Path) -> Option<&Path> {
    path.ancestors().find(|ancestor| {
        ancestor.join("idVendor").is_file() && ancestor.join("idProduct").is_file()
    })
}

pub(super) fn valid_hidraw_name(name: &str) -> bool {
    name.strip_prefix("hidraw").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn hid_id(uevent: &str) -> Option<(u16, u16)> {
    let id = uevent
        .lines()
        .find_map(|line| line.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':');
    let _bus = parts.next()?;
    let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
    let product = u32::from_str_radix(parts.next()?, 16).ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((u16::try_from(vendor).ok()?, u16::try_from(product).ok()?))
}

fn ancestor_string(path: &Path, name: &str) -> Option<String> {
    path.ancestors().find_map(|dir| {
        fs::read_to_string(dir.join(name)).ok().and_then(|text| {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
    })
}

fn ancestor_hex(path: &Path, name: &str) -> Option<u16> {
    ancestor_string(path, name).and_then(|text| {
        if let Some((major, minor)) = text.split_once('.') {
            let major = u16::from_str_radix(major, 16).ok()?;
            let minor = u16::from_str_radix(minor, 16).ok()?;
            (major <= 0xff && minor <= 0xff).then_some((major << 8) | minor)
        } else {
            u16::from_str_radix(text.trim_start_matches("0x"), 16).ok()
        }
    })
}

pub(super) fn descriptor_from_fd(fd: libc::c_int) -> Result<Vec<u8>> {
    let mut size = 0;
    if unsafe { libc::ioctl(fd, HIDIOCGRDESCSIZE, &mut size) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    if size <= 0 || size as usize >= MAX_DESCRIPTOR {
        return Err(format!("unsupported HID descriptor size {size}").into());
    }
    let mut descriptor = Box::new(ReportDescriptor {
        size: size as u32,
        value: [0; MAX_DESCRIPTOR],
    });
    if unsafe { libc::ioctl(fd, HIDIOCGRDESC, descriptor.as_mut()) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let actual = descriptor.size as usize;
    if actual == 0 || actual > MAX_DESCRIPTOR {
        return Err("hidraw returned an invalid descriptor size".into());
    }
    Ok(descriptor.value[..actual].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_reply_accepts_only_complete_unnumbered_shapes() {
        let mut payload = [0u8; HOST_REPORT_LEN];
        payload[..HOST_REPORT_LEN - 1]
            .iter_mut()
            .enumerate()
            .for_each(|(index, byte)| *byte = index as u8);
        let mut report = [0xff; HOST_REPORT_LEN];
        assert_eq!(
            normalize_unnumbered_feature_reply(&mut report, &payload, HOST_REPORT_LEN - 1).unwrap(),
            HOST_REPORT_LEN
        );
        assert_eq!(report[0], 0);
        assert_eq!(&report[1..], &payload[..HOST_REPORT_LEN - 1]);

        let mut numbered = [0u8; HOST_REPORT_LEN];
        numbered[1..].copy_from_slice(&payload[..HOST_REPORT_LEN - 1]);
        assert_eq!(
            normalize_unnumbered_feature_reply(&mut report, &numbered, HOST_REPORT_LEN).unwrap(),
            HOST_REPORT_LEN
        );
        assert_eq!(report, numbered);
        numbered[0] = 1;
        assert!(
            normalize_unnumbered_feature_reply(&mut report, &numbered, HOST_REPORT_LEN).is_err()
        );
        assert!(normalize_unnumbered_feature_reply(&mut report, &payload, 63).is_err());
    }

    #[test]
    fn parses_linux_identity_and_node_names() {
        assert_eq!(
            hid_id("HID_ID=0003:00003151:00004015\n"),
            Some((0x3151, 0x4015))
        );
        assert!(valid_hidraw_name("hidraw17"));
        assert!(!valid_hidraw_name("hidraw"));
        assert!(!valid_hidraw_name("hidraw2-other"));
    }
}
