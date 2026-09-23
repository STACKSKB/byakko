//! Windows HID collection inventory and guarded Nia87 feature access.
//!
//! This module uses the documented Windows HID and SetupAPI interfaces. It
//! contains no HIDAPI code. Inventory reads collection metadata only; feature
//! reports require the separate Nia87-validated `open` path.

use std::ffi::{CStr, CString};
use std::io;
use std::mem::{offset_of, size_of};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData, HidD_GetAttributes,
    HidD_GetFeature, HidD_GetHidGuid, HidD_GetPreparsedData, HidD_SetFeature, HidP_GetCaps,
    PHIDP_PREPARSED_DATA,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GENERIC_READ, GENERIC_WRITE,
    HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::core::GUID;

use super::{DeviceInfo, Result};

const HOST_REPORT_LEN: usize = 65;

struct InfoSet(HDEVINFO);

impl Drop for InfoSet {
    fn drop(&mut self) {
        // SAFETY: this handle was returned by SetupDiGetClassDevsW and is
        // released exactly once here.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

pub struct Device {
    handle: HANDLE,
}

// A Windows HANDLE can be used from the worker thread that owns a Device.
// The API calls here do not carry thread-local state; callers serialize their
// feature-report exchange at the device/protocol layer.
unsafe impl Send for Device {}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: this valid handle was returned by CreateFileW and is owned
        // exclusively by this Device.
        unsafe { CloseHandle(self.handle) };
    }
}

struct Preparsed(PHIDP_PREPARSED_DATA);

impl Drop for Preparsed {
    fn drop(&mut self) {
        // SAFETY: HidD_GetPreparsedData allocated this opaque pointer.
        unsafe { HidD_FreePreparsedData(self.0) };
    }
}

fn error(context: &str) -> Box<dyn std::error::Error + Send + Sync> {
    format!("{context}: {}", io::Error::last_os_error()).into()
}

fn wide_path(path: &CStr) -> Result<Vec<u16>> {
    let text = std::str::from_utf8(path.to_bytes())
        .map_err(|_| "Windows HID device path was not valid UTF-8")?;
    Ok(text.encode_utf16().chain(std::iter::once(0)).collect())
}

fn open_handle(path: &CStr, access: u32) -> Result<Device> {
    let path = wide_path(path)?;
    // SAFETY: path is zero-terminated and all other arguments are Win32
    // constants or null optional pointers. The returned handle is owned by
    // Device if valid.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(error("Could not open HID collection"))
    } else {
        Ok(Device { handle })
    }
}

fn attributes(handle: HANDLE) -> Result<HIDD_ATTRIBUTES> {
    let mut attributes = HIDD_ATTRIBUTES {
        Size: size_of::<HIDD_ATTRIBUTES>() as u32,
        ..HIDD_ATTRIBUTES::default()
    };
    // SAFETY: `attributes` is an initialized writable HIDD_ATTRIBUTES with
    // its Size field set to the structure size.
    if !unsafe { HidD_GetAttributes(handle, &mut attributes) } {
        return Err(error("Could not read HID collection attributes"));
    }
    Ok(attributes)
}

fn capabilities(handle: HANDLE) -> Result<HIDP_CAPS> {
    let mut pointer = 0;
    // SAFETY: pointer is a writable opaque preparsed-data pointer. The guard
    // below releases it on every later path.
    if !unsafe { HidD_GetPreparsedData(handle, &mut pointer) } {
        return Err(error("Could not read HID collection capabilities"));
    }
    let preparsed = Preparsed(pointer);
    let mut caps = HIDP_CAPS::default();
    // SAFETY: preparsed is valid until its guard drops and caps is writable.
    let status = unsafe { HidP_GetCaps(preparsed.0, &mut caps) };
    if status != HIDP_STATUS_SUCCESS {
        return Err(
            format!("Could not parse HID collection capabilities: status {status:#x}").into(),
        );
    }
    Ok(caps)
}

fn interface_number(path: &str) -> i32 {
    let lower = path.to_ascii_lowercase();
    lower
        .split("&mi_")
        .nth(1)
        .and_then(|tail| tail.get(..2))
        .and_then(|hex| i32::from_str_radix(hex, 16).ok())
        .unwrap_or(-1)
}

fn nia_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("vid_3151") && (lower.contains("pid_4011") || lower.contains("pid_4015"))
}

fn interface_path(set: HDEVINFO, interface: &SP_DEVICE_INTERFACE_DATA) -> Result<CString> {
    let mut needed = 0u32;
    // SAFETY: this sizing call intentionally passes a null output buffer.
    let first = unsafe {
        SetupDiGetDeviceInterfaceDetailW(set, interface, null_mut(), 0, &mut needed, null_mut())
    };
    if first != 0
        || io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
    {
        return Err(error("Could not size HID device interface path"));
    }
    let path_offset = offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
    if needed as usize <= path_offset + size_of::<u16>() {
        return Err("HID device interface path is too short".into());
    }
    // Vec<u32> supplies the alignment required by the Win32 detail structure.
    let mut buffer = vec![0u32; (needed as usize).div_ceil(size_of::<u32>())];
    let detail = buffer
        .as_mut_ptr()
        .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
    // SAFETY: the aligned allocation is at least `needed` bytes and the
    // structure's cbSize is initialized before SetupAPI writes the path.
    unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
    let mut written = 0u32;
    // SAFETY: detail points into the live buffer, sized in bytes as requested.
    if unsafe {
        SetupDiGetDeviceInterfaceDetailW(set, interface, detail, needed, &mut written, null_mut())
    } == 0
    {
        return Err(error("Could not read HID device interface path"));
    }
    let max_units = (needed as usize - path_offset) / size_of::<u16>();
    // SAFETY: DevicePath is a UTF-16 array within the returned output buffer.
    let path_units = unsafe {
        std::slice::from_raw_parts(
            (detail.cast::<u8>()).add(path_offset).cast::<u16>(),
            max_units,
        )
    };
    let end = path_units
        .iter()
        .position(|&unit| unit == 0)
        .ok_or("HID device interface path was not terminated")?;
    let text = String::from_utf16(&path_units[..end])?;
    Ok(CString::new(text)?)
}

pub fn enumerate() -> Result<Vec<DeviceInfo>> {
    let mut guid = GUID::default();
    // SAFETY: guid is writable; the function writes the HID interface GUID.
    unsafe { HidD_GetHidGuid(&mut guid) };
    // SAFETY: all optional arguments are null, and flags request present HID
    // device interfaces. InfoSet owns the returned handle.
    let raw_set = unsafe {
        SetupDiGetClassDevsW(
            &guid,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    // HDEVINFO is an isize in windows-sys, unlike HANDLE's pointer type.
    if raw_set == -1 {
        return Err(error("Could not enumerate HID collections"));
    }
    let set = InfoSet(raw_set);
    let mut found = Vec::new();
    let mut target_error = None;
    let mut index = 0;
    loop {
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..SP_DEVICE_INTERFACE_DATA::default()
        };
        // SAFETY: set/guid are valid and interface is writable with cbSize set.
        if unsafe { SetupDiEnumDeviceInterfaces(set.0, null(), &guid, index, &mut interface) } == 0
        {
            if io::Error::last_os_error().raw_os_error() == Some(ERROR_NO_MORE_ITEMS as i32) {
                break;
            }
            return Err(error("Could not enumerate HID device interface"));
        }
        index += 1;
        // An unrelated HID may disappear between enumeration and path lookup.
        let path = match interface_path(set.0, &interface) {
            Ok(path) => path,
            Err(_) => continue,
        };
        let lower = path.to_string_lossy().to_ascii_lowercase();
        let device = match open_handle(&path, 0) {
            Ok(device) => device,
            Err(error) => {
                if nia_path(&lower) {
                    target_error.get_or_insert_with(|| error.to_string());
                }
                continue;
            }
        };
        let attr = match attributes(device.handle) {
            Ok(attr) => attr,
            Err(error) => {
                if nia_path(&lower) {
                    target_error.get_or_insert_with(|| error.to_string());
                }
                continue;
            }
        };
        let caps = match capabilities(device.handle) {
            Ok(caps) => caps,
            Err(error) => {
                if attr.VendorID == 0x3151 && matches!(attr.ProductID, 0x4011 | 0x4015) {
                    target_error.get_or_insert_with(|| error.to_string());
                }
                continue;
            }
        };
        found.push(DeviceInfo {
            path,
            vid: attr.VendorID,
            pid: attr.ProductID,
            interface: interface_number(&lower),
            usage_page: caps.UsagePage,
            usage: caps.Usage,
            // Descriptive USB strings are optional and can block a live HID
            // transaction on this device. Identification uses attributes and
            // collection capabilities above instead.
            manufacturer: None,
            product: None,
            release: attr.VersionNumber,
        });
    }
    if let Some(error) = target_error
        && !found.iter().any(|device| {
            device.vid == 0x3151
                && matches!(device.pid, 0x4011 | 0x4015)
                && device.usage_page == 0xffff
                && device.usage == 2
        })
    {
        return Err(format!("Could not inspect a Nia87 HID collection: {error}").into());
    }
    Ok(found)
}

pub fn open(path: &CStr) -> Result<Device> {
    let device = open_handle(path, GENERIC_READ | GENERIC_WRITE)?;
    let attr = attributes(device.handle)?;
    let caps = capabilities(device.handle)?;
    if attr.VendorID != 0x3151
        || !matches!(attr.ProductID, 0x4011 | 0x4015)
        || caps.UsagePage != 0xffff
        || caps.Usage != 2
        || caps.FeatureReportByteLength as usize != HOST_REPORT_LEN
    {
        return Err("Opened HID path is not the expected Nia87 65-byte vendor collection".into());
    }
    Ok(device)
}

impl Device {
    pub fn send_feature_report(&self, data: &[u8]) -> Result<()> {
        if data.len() != HOST_REPORT_LEN || data[0] != 0 {
            return Err("Nia87 feature report must be 65 bytes with report ID zero".into());
        }
        // SAFETY: handle is open, and data points to a valid immutable buffer.
        if !unsafe { HidD_SetFeature(self.handle, data.as_ptr().cast(), data.len() as u32) } {
            return Err(error("Could not send HID feature report"));
        }
        Ok(())
    }

    pub fn get_feature_report(&self, data: &mut [u8]) -> Result<usize> {
        if data.len() != HOST_REPORT_LEN || data[0] != 0 {
            return Err("Nia87 feature report buffer must be 65 bytes with report ID zero".into());
        }
        // SAFETY: handle is open, and data points to a valid mutable buffer.
        if !unsafe { HidD_GetFeature(self.handle, data.as_mut_ptr().cast(), data.len() as u32) } {
            return Err(error("Could not read HID feature report"));
        }
        // HidD_GetFeature returns success/failure, not a byte count. The
        // collection capabilities were checked to require exactly 65 bytes.
        Ok(data.len())
    }

    pub fn get_report_descriptor(&self, _data: &mut [u8]) -> Result<usize> {
        Err(
            "Raw HID report descriptor is unavailable from the Windows user-mode collection API"
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{interface_number, nia_path};

    #[test]
    fn parses_windows_interface_number() {
        assert_eq!(interface_number(r"\\?\hid#vid_3151&pid_4015&mi_02#..."), 2);
        assert_eq!(interface_number(r"\\?\hid#vid_3151&pid_4015#..."), -1);
    }

    #[test]
    fn identifies_nia_paths_only_for_partial_inventory_errors() {
        assert!(nia_path(r"\\?\hid#vid_3151&pid_4015&mi_02#..."));
        assert!(nia_path(r"\\?\hid#VID_3151&PID_4011#..."));
        assert!(!nia_path(r"\\?\hid#vid_056a&pid_03f7#..."));
    }
}
