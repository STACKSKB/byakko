//! Linux hidraw enumeration and Nia87 configuration transport.
//!
//! ABI constants and report framing follow Linux's hidraw UAPI and hidraw
//! documentation. This is an independent implementation, not vendor code.

use std::{
    ffi::{CStr, CString, OsStr},
    fs::{self, File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::ffi::OsStrExt},
    path::{Path, PathBuf},
};

use super::{DeviceInfo, Result};

const MAX_DESCRIPTOR: usize = 4096;
const HOST_REPORT_LEN: usize = 65;
const HIDRAW_CLASS: &str = "/sys/class/hidraw";

// Linux asm-generic/ioctl.h encodes direction, size, type, and number in these
// bit positions. hidraw.h defines the 'H' ioctl numbers used below.
const fn ioc(direction: u32, number: u32, size: u32) -> libc::c_ulong {
    ((direction << 30) | (size << 16) | ((b'H' as u32) << 8) | number) as libc::c_ulong
}
const IOC_READ: u32 = 2;
const IOC_WRITE: u32 = 1;
const HIDIOCGRDESCSIZE: libc::c_ulong = ioc(IOC_READ, 0x01, 4);
const HIDIOCGRDESC: libc::c_ulong = ioc(IOC_READ, 0x02, 4 + MAX_DESCRIPTOR as u32);
const HIDIOCGRAWINFO: libc::c_ulong = ioc(IOC_READ, 0x03, 8);

#[repr(C)]
struct ReportDescriptor {
    size: u32,
    value: [u8; MAX_DESCRIPTOR],
}

#[repr(C)]
#[derive(Default)]
struct RawInfo {
    bus_type: u32,
    vendor: i16,
    product: i16,
}

pub struct Device {
    file: File,
    numbered_feature_reports: bool,
}

impl Device {
    pub fn send_feature_report(&self, report: &[u8]) -> Result<()> {
        if report.len() != HOST_REPORT_LEN || report[0] != 0 {
            return Err("Nia87 feature report must be 65 bytes with report ID zero".into());
        }
        let mut buffer = report.to_vec();
        let request = ioc(IOC_READ | IOC_WRITE, 0x06, buffer.len() as u32);
        // SAFETY: buffer is valid and mutable for `len` bytes. The hidraw
        // ioctl only accesses it during this synchronous call.
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
        buffer[0] = report[0]; // requested report ID; zero for the observed Nia87.
        let request = ioc(IOC_READ | IOC_WRITE, 0x07, buffer.len() as u32);
        // SAFETY: buffer is valid and mutable for `len` bytes for the entire
        // synchronous ioctl call.
        let status = unsafe { libc::ioctl(self.file.as_raw_fd(), request, buffer.as_mut_ptr()) };
        if status < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let count = status as usize;
        if count == 0 || count > buffer.len() {
            return Err("hidraw returned an invalid feature reply length".into());
        }
        if self.numbered_feature_reports {
            report[..count].copy_from_slice(&buffer[..count]);
            Ok(count)
        } else {
            // hidraw places an unnumbered payload at byte zero. The existing
            // device layer uses hidapi's host-buffer convention: report ID 0
            // at byte zero, followed by the 64-byte device payload.
            if count >= report.len() {
                return Err("unnumbered feature reply does not fit host buffer".into());
            }
            report[0] = 0;
            report[1..=count].copy_from_slice(&buffer[..count]);
            Ok(count + 1)
        }
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
    // SAFETY: RawInfo has the UAPI struct layout and is writable for the
    // duration of the synchronous ioctl.
    let status = unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCGRAWINFO, &mut info) };
    if status < 0 {
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
    Ok(Device {
        file,
        numbered_feature_reports: parsed.numbered_feature_reports,
    })
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
        let canonical = fs::canonicalize(&sys_device).unwrap_or(sys_device.clone());
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
        let path = CString::new(dev_path.as_os_str().as_bytes())?;
        found.push(DeviceInfo {
            path,
            vid,
            pid,
            interface,
            usage_page: parsed
                .and_then(|parsed| parsed.application_usage)
                .map_or(0, |usage| usage.0),
            usage: parsed
                .and_then(|parsed| parsed.application_usage)
                .map_or(0, |usage| usage.1),
            manufacturer,
            product,
            release,
        });
    }
    found.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(found)
}

fn valid_hidraw_name(name: &str) -> bool {
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

fn descriptor_from_fd(fd: libc::c_int) -> Result<Vec<u8>> {
    let mut size: libc::c_int = 0;
    // SAFETY: size is a writable C int for the synchronous ioctl call.
    let status = unsafe { libc::ioctl(fd, HIDIOCGRDESCSIZE, &mut size) };
    if status < 0 {
        return Err(io::Error::last_os_error().into());
    }
    if size <= 0 || size as usize >= MAX_DESCRIPTOR {
        return Err(format!("unsupported HID descriptor size {size}").into());
    }
    let mut descriptor = Box::new(ReportDescriptor {
        size: size as u32,
        value: [0; MAX_DESCRIPTOR],
    });
    // SAFETY: descriptor has the hidraw_report_descriptor UAPI layout and
    // its size field bounds writes to its 4096-byte value buffer.
    let status = unsafe { libc::ioctl(fd, HIDIOCGRDESC, descriptor.as_mut()) };
    if status < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let actual = descriptor.size as usize;
    if actual == 0 || actual > MAX_DESCRIPTOR {
        return Err("hidraw returned an invalid descriptor size".into());
    }
    Ok(descriptor.value[..actual].to_vec())
}

#[derive(Clone, Copy, Debug)]
struct DescriptorInfo {
    target: bool,
    numbered_feature_reports: bool,
    application_usage: Option<(u16, u16)>,
}

/// Parse HID items and inspect only top-level Application Collections. Usage
/// values inside nested collections or ordinary feature fields cannot qualify.
fn parse_descriptor(bytes: &[u8]) -> Option<DescriptorInfo> {
    let mut position = 0;
    let mut usage_page = 0u32;
    let mut global_stack = Vec::new();
    let mut local_usage = None;
    let mut local_minimum = None;
    let mut depth = 0usize;
    let mut target = false;
    let mut numbered_feature_reports = false;
    let mut application_usage = None;
    while position < bytes.len() {
        let prefix = bytes[position];
        position += 1;
        if prefix == 0xfe {
            if position + 2 > bytes.len() {
                return None;
            }
            let length = bytes[position] as usize;
            position += 2; // size and long-item tag
            position = position.checked_add(length)?;
            if position > bytes.len() {
                return None;
            }
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            value => value as usize,
        };
        let end = position.checked_add(size)?;
        let data = bytes.get(position..end)?;
        position = end;
        let value = data.iter().enumerate().fold(0u32, |value, (shift, byte)| {
            value | (u32::from(*byte) << (shift * 8))
        });
        let item_type = (prefix >> 2) & 3;
        let tag = prefix >> 4;
        match (item_type, tag) {
            (1, 0) => usage_page = value,                // Global: Usage Page
            (1, 8) => numbered_feature_reports = true,   // Global: Report ID
            (1, 10) => global_stack.push(usage_page),    // Global: Push
            (1, 11) => usage_page = global_stack.pop()?, // Global: Pop
            (2, 0) => {
                local_usage.get_or_insert(full_usage(usage_page, value, size));
            }
            (2, 1) => local_minimum = Some(full_usage(usage_page, value, size)),
            (0, 10) => {
                // Main: Collection
                let usage = local_usage.or(local_minimum);
                if depth == 0
                    && value == 1
                    && let Some(usage) = usage
                {
                    let page = (usage >> 16) as u16;
                    let usage = usage as u16;
                    if page == 0xffff && usage == 2 {
                        target = true;
                        // Keep the Nia collection discoverable even if a
                        // descriptor has another application collection first.
                        application_usage = Some((page, usage));
                    } else if application_usage.is_none() {
                        application_usage = Some((page, usage));
                    }
                }
                depth += 1;
                local_usage = None;
                local_minimum = None;
            }
            (0, 12) => {
                // Main: End Collection
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                local_usage = None;
                local_minimum = None;
            }
            (0, _) => {
                local_usage = None;
                local_minimum = None;
            }
            _ => {}
        }
    }
    (depth == 0 && global_stack.is_empty()).then_some(DescriptorInfo {
        target,
        numbered_feature_reports,
        application_usage,
    })
}

fn full_usage(page: u32, value: u32, size: usize) -> u32 {
    if size == 4 {
        value
    } else {
        ((page & 0xffff) << 16) | value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_nia87_descriptor_is_top_level_vendor_usage() {
        let descriptor = [
            0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01, 0x09, 0x02, 0x15, 0x80, 0x25, 0x7f, 0x75,
            0x08, 0x95, 0x40, 0xb1, 0x02, 0xc0,
        ];
        let parsed = parse_descriptor(&descriptor).unwrap();
        assert!(parsed.target);
        assert!(!parsed.numbered_feature_reports);
    }

    #[test]
    fn nested_or_plain_vendor_usage_does_not_select_collection() {
        let nested = [
            0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01, 0xc0,
            0xc0,
        ];
        let plain = [0x06, 0xff, 0xff, 0x09, 0x02, 0xb1, 0x02];
        assert!(!parse_descriptor(&nested).unwrap().target);
        assert!(!parse_descriptor(&plain).unwrap().target);
    }

    #[test]
    fn parser_reports_non_nia_top_level_application_usages() {
        let keyboard = [0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0xc0];
        let parsed = parse_descriptor(&keyboard).unwrap();
        assert_eq!(parsed.application_usage, Some((0x0001, 0x0006)));
        assert!(!parsed.target);

        let consumer = [0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0xc0];
        let parsed = parse_descriptor(&consumer).unwrap();
        assert_eq!(parsed.application_usage, Some((0x000c, 0x0001)));
        assert!(!parsed.target);
    }

    #[test]
    fn nia_target_usage_is_preferred_if_another_application_comes_first() {
        let mixed = [
            0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0xc0, 0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01,
            0xc0,
        ];
        let parsed = parse_descriptor(&mixed).unwrap();
        assert!(parsed.target);
        assert_eq!(parsed.application_usage, Some((0xffff, 0x0002)));
    }

    #[test]
    fn parser_tracks_global_stack_and_rejects_truncation() {
        let pushed = [
            0x06, 0xff, 0xff, 0xa4, 0x05, 0x01, 0xb4, 0x09, 0x02, 0xa1, 0x01, 0xc0,
        ];
        assert!(parse_descriptor(&pushed).unwrap().target);
        assert!(parse_descriptor(&pushed[..pushed.len() - 1]).is_none());
        assert!(parse_descriptor(&[0x06, 0xff]).is_none());
    }

    #[test]
    fn parses_sysfs_identity_and_device_node_names() {
        assert_eq!(
            hid_id("HID_ID=0003:00003151:00004015\n"),
            Some((0x3151, 0x4015))
        );
        assert!(valid_hidraw_name("hidraw17"));
        assert!(!valid_hidraw_name("hidraw"));
        assert!(!valid_hidraw_name("hidraw2-other"));
    }
}
