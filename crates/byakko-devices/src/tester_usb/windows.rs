// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only USB descriptor metadata for one selected Windows Raw Input keyboard.
//!
//! The Raw Input path is resolved as an exact device interface, then its devnode is
//! walked to a physical USB devnode. The latter's driver key must identify one
//! hub port before a standard GET_DESCRIPTOR is sent through that hub.

use super::{Metadata, Speed, Unavailable};
use std::{
    io,
    mem::{offset_of, size_of, size_of_val},
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Devices::{
            DeviceAndDriverInstallation::{
                CM_DRP_DRIVER, CM_Get_DevNode_Registry_PropertyW, CM_Get_Device_IDW, CM_Get_Parent,
                CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO,
                SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
                SPINT_ACTIVE, SetupDiCreateDeviceInfoList, SetupDiDestroyDeviceInfoList,
                SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
                SetupDiGetDeviceInterfaceDetailW, SetupDiOpenDeviceInterfaceW,
            },
            Usb::{
                DeviceConnected, GUID_DEVINTERFACE_USB_HUB,
                IOCTL_USB_GET_DESCRIPTOR_FROM_NODE_CONNECTION,
                IOCTL_USB_GET_NODE_CONNECTION_DRIVERKEY_NAME,
                IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX,
                IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2, IOCTL_USB_GET_NODE_INFORMATION,
                USB_CONFIGURATION_DESCRIPTOR_TYPE, USB_NODE_CONNECTION_INFORMATION_EX,
                USB_NODE_CONNECTION_INFORMATION_EX_V2, USB_NODE_INFORMATION, USB_PIPE_INFO,
                UsbFullSpeed, UsbHighSpeed, UsbHub, UsbLowSpeed,
            },
        },
        Foundation::{
            CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GENERIC_WRITE, HANDLE,
            INVALID_HANDLE_VALUE,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
        UI::Input::{
            GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICENAME,
            RIM_TYPEKEYBOARD,
        },
    },
    core::GUID,
};

const MAX_RAW_DEVICES: u32 = 4096;
const MAX_HUBS: u32 = 128;
const MAX_DESCRIPTOR_BYTES: usize = 4096;
const MAX_CONFIGURATIONS: u8 = 16;
const DRIVER_KEY_BYTES: usize = 1024;

struct InfoSet(HDEVINFO);
impl Drop for InfoSet {
    fn drop(&mut self) {
        // SAFETY: the guard owns this SetupDiGetClassDevsW result.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

struct Hub(HANDLE);
impl Drop for Hub {
    fn drop(&mut self) {
        // SAFETY: the guard owns this CreateFileW result.
        unsafe { CloseHandle(self.0) };
    }
}

fn unavailable(message: impl Into<String>) -> Unavailable {
    Unavailable(message.into())
}

fn last_error(context: &str) -> Unavailable {
    unavailable(format!("{context}: {}", io::Error::last_os_error()))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn raw_name(handle: HANDLE) -> Option<String> {
    let mut length = 0;
    // SAFETY: a null buffer requests the required UTF-16 character count.
    if unsafe { GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, null_mut(), &mut length) } != 0
        || length == 0
        || length > 4096
    {
        return None;
    }
    let mut units = vec![0u16; length as usize];
    // SAFETY: units holds length UTF-16 characters.
    if unsafe {
        GetRawInputDeviceInfoW(
            handle,
            RIDI_DEVICENAME,
            units.as_mut_ptr().cast(),
            &mut length,
        )
    } == u32::MAX
    {
        return None;
    }
    let end = units.iter().position(|&unit| unit == 0)?;
    String::from_utf16(&units[..end]).ok()
}

fn selected_raw_input(id: &str) -> Result<(), Unavailable> {
    let mut count = 0;
    let entry_size = size_of::<RAWINPUTDEVICELIST>() as u32;
    // SAFETY: the first call requests the current inventory size.
    if unsafe { GetRawInputDeviceList(null_mut(), &mut count, entry_size) } == u32::MAX
        || count > MAX_RAW_DEVICES
    {
        return Err(unavailable(
            "Selected Raw Input keyboard inventory is unavailable",
        ));
    }
    let mut entries = vec![RAWINPUTDEVICELIST::default(); count as usize];
    // SAFETY: entries is sized for the returned count.
    let read = unsafe { GetRawInputDeviceList(entries.as_mut_ptr(), &mut count, entry_size) };
    if read == u32::MAX || read > count {
        return Err(last_error("Could not inspect selected Raw Input keyboard"));
    }
    let matches = entries
        .into_iter()
        .take(read as usize)
        .filter(|entry| {
            entry.dwType == RIM_TYPEKEYBOARD && raw_name(entry.hDevice).as_deref() == Some(id)
        })
        .count();
    match matches {
        1 => Ok(()),
        0 => Err(unavailable(
            "Selected Raw Input keyboard is no longer present",
        )),
        _ => Err(unavailable(
            "Selected Raw Input keyboard identity is ambiguous",
        )),
    }
}

fn info_set(guid: &GUID) -> Result<InfoSet, Unavailable> {
    // SAFETY: optional parameters are null and the result is owned by InfoSet.
    let set = unsafe {
        SetupDiGetClassDevsW(
            guid,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if set == -1 {
        Err(last_error("Could not enumerate Windows device interfaces"))
    } else {
        Ok(InfoSet(set))
    }
}

fn interface_detail(
    set: HDEVINFO,
    interface: &SP_DEVICE_INTERFACE_DATA,
) -> Result<(String, u32), Unavailable> {
    let mut needed = 0;
    // SAFETY: null detail buffer requests its size.
    let first = unsafe {
        SetupDiGetDeviceInterfaceDetailW(set, interface, null_mut(), 0, &mut needed, null_mut())
    };
    if first != 0
        || io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
        || needed < size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32
        || needed > 8192
    {
        return Err(unavailable("Could not size Windows device interface path"));
    }
    let mut buffer = vec![0u32; (needed as usize).div_ceil(size_of::<u32>())];
    let detail = buffer
        .as_mut_ptr()
        .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
    // SAFETY: the aligned allocation is large enough for the detail structure.
    unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
    let mut info = SP_DEVINFO_DATA {
        cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    // SAFETY: detail and info are initialized writable outputs.
    if unsafe {
        SetupDiGetDeviceInterfaceDetailW(set, interface, detail, needed, null_mut(), &mut info)
    } == 0
    {
        return Err(last_error("Could not read Windows device interface path"));
    }
    let offset = offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
    let units = (needed as usize - offset) / size_of::<u16>();
    // SAFETY: DevicePath lies in the live detail buffer.
    let path = unsafe {
        std::slice::from_raw_parts((detail.cast::<u8>()).add(offset).cast::<u16>(), units)
    };
    let end = path
        .iter()
        .position(|&unit| unit == 0)
        .ok_or_else(|| unavailable("Windows device interface path is unterminated"))?;
    let path = String::from_utf16(&path[..end])
        .map_err(|_| unavailable("Windows device interface path is invalid UTF-16"))?;
    Ok((path, info.DevInst))
}

fn interfaces(guid: &GUID, limit: u32) -> Result<Vec<(String, u32)>, Unavailable> {
    let set = info_set(guid)?;
    let mut found = Vec::new();
    for index in 0..limit {
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: set and guid are valid; interface has the required cbSize.
        if unsafe { SetupDiEnumDeviceInterfaces(set.0, null(), guid, index, &mut interface) } == 0 {
            if io::Error::last_os_error().raw_os_error() == Some(ERROR_NO_MORE_ITEMS as i32) {
                return Ok(found);
            }
            return Err(last_error("Could not enumerate Windows device interface"));
        }
        // An unrelated device may disappear during enumeration.
        if let Ok(detail) = interface_detail(set.0, &interface) {
            found.push(detail);
        }
    }
    Err(unavailable(
        "Windows device interface inventory exceeded the safety bound",
    ))
}

fn selected_node(path: &str) -> Result<u32, Unavailable> {
    // Raw Input supplies an exact device-interface path. Keyboard interfaces
    // can use an interface class other than GUID_DEVINTERFACE_HID.
    // SAFETY: null class creates an empty local device information set.
    let raw = unsafe { SetupDiCreateDeviceInfoList(null(), null_mut()) };
    if raw == -1 {
        return Err(last_error(
            "Could not create selected device information set",
        ));
    }
    let set = InfoSet(raw);
    let mut interface = SP_DEVICE_INTERFACE_DATA {
        cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
        ..Default::default()
    };
    let path_units = wide(path);
    // SAFETY: exact selected path is terminated; interface is writable.
    if unsafe { SetupDiOpenDeviceInterfaceW(set.0, path_units.as_ptr(), 0, &mut interface) } == 0 {
        return Err(last_error(
            "Selected Raw Input device interface is unavailable",
        ));
    }
    if interface.Flags & SPINT_ACTIVE == 0 {
        return Err(unavailable(
            "Selected Raw Input device interface is inactive",
        ));
    }
    let (resolved, node) = interface_detail(set.0, &interface)?;
    if !resolved.eq_ignore_ascii_case(path) {
        return Err(unavailable("Selected Raw Input device interface changed"));
    }
    Ok(node)
}
fn device_id(node: u32) -> Result<String, Unavailable> {
    let mut units = [0u16; 512];
    // SAFETY: node is a devnode returned by SetupAPI or CM_Get_Parent.
    if unsafe { CM_Get_Device_IDW(node, units.as_mut_ptr(), units.len() as u32, 0) } != CR_SUCCESS {
        return Err(unavailable("Could not read selected device instance ID"));
    }
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .ok_or_else(|| unavailable("Selected device instance ID is too long"))?;
    String::from_utf16(&units[..end])
        .map_err(|_| unavailable("Selected device instance ID is invalid UTF-16"))
}

fn physical_usb(mut node: u32) -> Result<(u32, Option<u8>), Unavailable> {
    let mut interface = None;
    for _ in 0..32 {
        let id = device_id(node)?.to_ascii_uppercase();
        if let Some(hardware) = id.strip_prefix("USB\\") {
            let hardware = hardware.split('\\').next().unwrap_or("");
            if hardware.starts_with("VID_") && hardware.contains("&PID_") {
                if let Some(mi) = hardware.split("&MI_").nth(1) {
                    let number = mi
                        .get(..2)
                        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                        .ok_or_else(|| unavailable("Selected USB interface number is invalid"))?;
                    if interface.replace(number).is_some() {
                        return Err(unavailable("Selected USB interface ancestry is ambiguous"));
                    }
                } else {
                    return Ok((node, interface));
                }
            }
        }
        let mut parent = 0;
        // SAFETY: parent is writable and node is a valid devnode.
        if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
            break;
        }
        node = parent;
    }
    Err(unavailable(
        "Selected keyboard has no unambiguous physical USB ancestor",
    ))
}

fn driver_key(node: u32) -> Result<String, Unavailable> {
    let mut kind = 0;
    let mut units = [0u16; 512];
    let mut bytes = size_of_val(&units) as u32;
    // SAFETY: fixed writable buffer and type/length outputs; no registry mutation.
    if unsafe {
        CM_Get_DevNode_Registry_PropertyW(
            node,
            CM_DRP_DRIVER,
            &mut kind,
            units.as_mut_ptr().cast(),
            &mut bytes,
            0,
        )
    } != CR_SUCCESS
        || kind != 1
        || bytes < 4
        || bytes as usize > size_of_val(&units)
    {
        return Err(unavailable("Could not read physical USB device driver key"));
    }
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .ok_or_else(|| unavailable("Physical USB driver key is unterminated"))?;
    if end == 0 {
        return Err(unavailable("Physical USB driver key is empty"));
    }
    String::from_utf16(&units[..end])
        .map(|text| text.to_ascii_uppercase())
        .map_err(|_| unavailable("Physical USB driver key is invalid UTF-16"))
}

fn open_hub(path: &str) -> Result<Hub, Unavailable> {
    let path = wide(path);
    // SAFETY: path is terminated. USBView opens hubs with GENERIC_WRITE;
    // this handle is used only for read-only hub and GET_DESCRIPTOR requests.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(last_error("Could not open USB hub for descriptor query"))
    } else {
        Ok(Hub(handle))
    }
}

fn ioctl<T>(hub: &Hub, code: u32, data: &mut T) -> Result<u32, Unavailable> {
    let mut returned = 0;
    // SAFETY: initialized struct is both input and writable output of identical size.
    let ok = unsafe {
        DeviceIoControl(
            hub.0,
            code,
            (data as *mut T).cast(),
            size_of::<T>() as u32,
            (data as *mut T).cast(),
            size_of::<T>() as u32,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 {
        Err(last_error("USB hub metadata request failed"))
    } else {
        Ok(returned)
    }
}

fn hub_port_count(hub: &Hub) -> Result<u8, Unavailable> {
    let mut info = USB_NODE_INFORMATION {
        NodeType: UsbHub,
        ..Default::default()
    };
    if ioctl(hub, IOCTL_USB_GET_NODE_INFORMATION, &mut info)?
        < size_of::<USB_NODE_INFORMATION>() as u32
    {
        return Err(unavailable("USB hub information was truncated"));
    }
    if info.NodeType != UsbHub {
        return Err(unavailable("USB interface does not describe a hub"));
    }
    // SAFETY: NodeType identifies the HubInformation union member.
    let count = unsafe { info.u.HubInformation.HubDescriptor.bNumberOfPorts };
    if count == 0 {
        return Err(unavailable("USB hub port count is invalid"));
    }
    Ok(count)
}

fn port_driver_key(hub: &Hub, port: u32) -> Option<String> {
    // USB_NODE_CONNECTION_DRIVERKEY_NAME is an 8-byte header followed by UTF-16.
    let mut buffer = vec![0u32; DRIVER_KEY_BYTES.div_ceil(4)];
    buffer[0] = port;
    let mut returned = 0;
    // SAFETY: aligned buffer has a writable byte length and initialized port index.
    let ok = unsafe {
        DeviceIoControl(
            hub.0,
            IOCTL_USB_GET_NODE_CONNECTION_DRIVERKEY_NAME,
            buffer.as_ptr().cast(),
            DRIVER_KEY_BYTES as u32,
            buffer.as_mut_ptr().cast(),
            DRIVER_KEY_BYTES as u32,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 || returned < 10 {
        return None;
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), DRIVER_KEY_BYTES) };
    let actual = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    if !(10..=DRIVER_KEY_BYTES).contains(&actual) || actual > returned as usize {
        return None;
    }
    let units: Vec<u16> = bytes[8..actual]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let end = units.iter().position(|&unit| unit == 0)?;
    String::from_utf16(&units[..end])
        .ok()
        .map(|text| text.to_ascii_uppercase())
}

fn matched_port(key: &str) -> Result<(Hub, u32), Unavailable> {
    let mut match_port = None;
    for (path, _) in interfaces(&GUID_DEVINTERFACE_USB_HUB, MAX_HUBS)? {
        let Ok(hub) = open_hub(&path) else { continue };
        let Ok(ports) = hub_port_count(&hub) else {
            continue;
        };
        for port in 1..=u32::from(ports) {
            if port_driver_key(&hub, port).as_deref() == Some(key) {
                if match_port.is_some() {
                    return Err(unavailable(
                        "Physical USB device maps to multiple hub ports",
                    ));
                }
                match_port = Some((path.clone(), port));
                break;
            }
        }
    }
    let (path, port) = match_port
        .ok_or_else(|| unavailable("Physical USB device has no accessible matching hub port"))?;
    let hub = open_hub(&path)?;
    if port_driver_key(&hub, port).as_deref() != Some(key) {
        return Err(unavailable(
            "Physical USB device changed after hub port lookup",
        ));
    }
    Ok((hub, port))
}

fn port_connection(hub: &Hub, port: u32) -> Result<(u8, u8, u8), Unavailable> {
    // USB endpoint numbers 1..=15 each allow IN and OUT, hence at most 30
    // pipes. The Windows USBView sample allocates this bound for the EX reply.
    const MAX_PIPES: usize = 30;
    let header = offset_of!(USB_NODE_CONNECTION_INFORMATION_EX, PipeList);
    let bytes =
        size_of::<USB_NODE_CONNECTION_INFORMATION_EX>() + MAX_PIPES * size_of::<USB_PIPE_INFO>();
    let mut buffer = vec![0u32; bytes.div_ceil(4)];
    let info = buffer
        .as_mut_ptr()
        .cast::<USB_NODE_CONNECTION_INFORMATION_EX>();
    // SAFETY: the aligned buffer is at least bytes long, and ConnectionIndex
    // is its first field. DeviceIoControl writes the same live buffer.
    let mut returned = 0;
    let ok = unsafe {
        (*info).ConnectionIndex = port;
        DeviceIoControl(
            hub.0,
            IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX,
            info.cast(),
            bytes as u32,
            info.cast(),
            bytes as u32,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 {
        return Err(last_error("USB port connection information request failed"));
    }
    if returned < header as u32 || returned as usize > bytes {
        return Err(unavailable("USB port connection information was truncated"));
    }
    // SAFETY: the returned header is present and info points into the buffer.
    let info = unsafe { &*info };
    let pipes = info.NumberOfOpenPipes as usize;
    if info.ConnectionIndex != port
        || pipes > MAX_PIPES
        || (returned as usize) < header + pipes * size_of::<USB_PIPE_INFO>()
    {
        return Err(unavailable(
            "USB port connection information was inconsistent",
        ));
    }
    if info.ConnectionStatus != DeviceConnected
        || info.DeviceIsHub
        || info.CurrentConfigurationValue == 0
    {
        return Err(unavailable(
            "Selected USB hub port is not an active keyboard device",
        ));
    }
    let speed = if i32::from(info.Speed) == UsbLowSpeed {
        0
    } else if i32::from(info.Speed) == UsbFullSpeed {
        1
    } else if i32::from(info.Speed) == UsbHighSpeed {
        2
    } else {
        return Err(unavailable("Selected USB speed is unsupported"));
    };
    Ok((
        speed,
        info.CurrentConfigurationValue,
        info.DeviceDescriptor.bNumConfigurations,
    ))
}
fn operating_speed(hub: &Hub, port: u32, base: u8) -> Result<Speed, Unavailable> {
    match base {
        0 => Ok(Speed::Low),
        1 => Ok(Speed::Full),
        2 => {
            let mut info = USB_NODE_CONNECTION_INFORMATION_EX_V2 {
                ConnectionIndex: port,
                Length: size_of::<USB_NODE_CONNECTION_INFORMATION_EX_V2>() as u32,
                ..Default::default()
            };
            // Ask explicitly about USB 3.0. The returned flag describes the
            // operating speed, rather than merely the port capability.
            info.SupportedUsbProtocols.ul = 1 << 2;
            let returned = ioctl(
                hub,
                IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2,
                &mut info,
            )
            .map_err(|_| unavailable("Could not distinguish high-speed from SuperSpeed USB"))?;
            if returned < size_of::<USB_NODE_CONNECTION_INFORMATION_EX_V2>() as u32 {
                return Err(unavailable("USB port speed information was truncated"));
            }
            // SAFETY: the hub wrote the flags union. Bit 0 is operating at SuperSpeed.
            let flags = unsafe { info.Flags.ul };
            if flags & 1 != 0 {
                Ok(Speed::Super)
            } else {
                Ok(Speed::High)
            }
        }
        _ => Err(unavailable("Selected USB speed is unsupported")),
    }
}

fn configuration_descriptor(
    hub: &Hub,
    port: u32,
    index: u8,
    length: usize,
) -> Result<Vec<u8>, Unavailable> {
    // USB_DESCRIPTOR_REQUEST has a 12-byte fixed header and a trailing Data field.
    let mut buffer = vec![0u32; (12 + length).div_ceil(4)];
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), buffer.len() * 4)
    };
    bytes[0..4].copy_from_slice(&port.to_le_bytes());
    bytes[4] = 0x80; // standard device-to-host GET_DESCRIPTOR
    bytes[5] = 6;
    bytes[6..8].copy_from_slice(
        &(((USB_CONFIGURATION_DESCRIPTOR_TYPE as u16) << 8) | u16::from(index)).to_le_bytes(),
    );
    bytes[10..12].copy_from_slice(&(length as u16).to_le_bytes());
    let mut returned = 0;
    // SAFETY: same live buffer supplies the documented request and descriptor output.
    let ok = unsafe {
        DeviceIoControl(
            hub.0,
            IOCTL_USB_GET_DESCRIPTOR_FROM_NODE_CONNECTION,
            bytes.as_ptr().cast(),
            (12 + length) as u32,
            bytes.as_mut_ptr().cast(),
            (12 + length) as u32,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 || returned < (12 + length) as u32 {
        return Err(last_error(
            "Could not read selected USB configuration descriptor",
        ));
    }
    Ok(bytes[12..12 + length].to_vec())
}

fn active_configuration(
    hub: &Hub,
    port: u32,
    value: u8,
    count: u8,
) -> Result<Vec<u8>, Unavailable> {
    if count == 0 || count > MAX_CONFIGURATIONS {
        return Err(unavailable(
            "Selected USB configuration count is unsupported",
        ));
    }
    let mut found = None;
    for index in 0..count {
        let header = configuration_descriptor(hub, port, index, 9)?;
        if header.len() != 9 || header[0] != 9 || header[1] != 2 {
            return Err(unavailable(
                "Selected USB configuration descriptor header is invalid",
            ));
        }
        if header[5] != value {
            continue;
        }
        if found.is_some() {
            return Err(unavailable("Selected USB configuration value is ambiguous"));
        }
        let length = u16::from_le_bytes([header[2], header[3]]) as usize;
        if !(9..=MAX_DESCRIPTOR_BYTES).contains(&length) {
            return Err(unavailable(
                "Selected USB configuration descriptor size is invalid",
            ));
        }
        found = Some(configuration_descriptor(hub, port, index, length)?);
    }
    found.ok_or_else(|| unavailable("Active USB configuration descriptor was not found"))
}

fn selected_interface(descriptors: &[u8], mi: Option<u8>) -> Result<u8, Unavailable> {
    if let Some(mi) = mi {
        return Ok(mi);
    }
    // A non-composite USB devnode has no MI number. Read the sole interface's
    // actual bInterfaceNumber; it need not be zero.
    if descriptors.get(4) != Some(&1) {
        return Err(unavailable(
            "Selected HID collection has no unambiguous USB interface",
        ));
    }
    let mut cursor = 0;
    let mut selected = None;
    while cursor + 2 <= descriptors.len() {
        let length = usize::from(descriptors[cursor]);
        if length < 2 || cursor + length > descriptors.len() {
            return Err(unavailable("Selected USB descriptor stream is malformed"));
        }
        if descriptors[cursor + 1] == 4 {
            if length < 9 {
                return Err(unavailable(
                    "Selected USB interface descriptor is malformed",
                ));
            }
            let number = descriptors[cursor + 2];
            if selected.is_some_and(|current| current != number) {
                return Err(unavailable("Selected USB interface number is ambiguous"));
            }
            selected = Some(number);
        }
        cursor += length;
    }
    if cursor != descriptors.len() {
        return Err(unavailable("Selected USB descriptor stream is truncated"));
    }
    selected.ok_or_else(|| unavailable("Selected USB interface descriptor was not found"))
}
fn single_alternate(descriptors: &[u8], interface: u8) -> Result<u8, Unavailable> {
    let mut cursor = 0;
    let mut alternate = None;
    while cursor + 2 <= descriptors.len() {
        let length = usize::from(descriptors[cursor]);
        if length < 2 || cursor + length > descriptors.len() {
            return Err(unavailable("Selected USB descriptor stream is malformed"));
        }
        if descriptors[cursor + 1] == 4 && length >= 9 && descriptors[cursor + 2] == interface {
            let value = descriptors[cursor + 3];
            if alternate.replace(value).is_some() {
                return Err(unavailable(
                    "Active USB alternate setting cannot be established",
                ));
            }
        }
        cursor += length;
    }
    if cursor != descriptors.len() {
        return Err(unavailable("Selected USB descriptor stream is truncated"));
    }
    alternate.ok_or_else(|| unavailable("Selected USB interface descriptor was not found"))
}

pub fn inspect(device: &crate::tester_input::InputDevice) -> Result<Metadata, Unavailable> {
    if device.id != device.identity || device.id.is_empty() {
        return Err(unavailable("Selected Raw Input keyboard identity changed"));
    }
    selected_raw_input(&device.id)?;

    let node = selected_node(&device.id)?;
    let (usb_node, mi) = physical_usb(node)?;
    let key = driver_key(usb_node)?;
    let (hub, port) = matched_port(&key)?;
    // The port key is checked again before and after descriptor reads to catch
    // a device replacement during the read-only inspection.
    if port_driver_key(&hub, port).as_deref() != Some(&key) {
        return Err(unavailable(
            "Selected USB hub port changed during inspection",
        ));
    }
    let (base_speed, value, count) = port_connection(&hub, port)?;
    let speed = operating_speed(&hub, port, base_speed)?;
    let descriptors = active_configuration(&hub, port, value, count)?;
    let interface = selected_interface(&descriptors, mi)?;
    let alternate = single_alternate(&descriptors, interface)?;
    let endpoints = super::interrupt_endpoints(&descriptors, value, interface, alternate, speed)?;
    if port_connection(&hub, port)? != (base_speed, value, count)
        || operating_speed(&hub, port, base_speed)? != speed
    {
        return Err(unavailable(
            "Selected USB configuration or speed changed during inspection",
        ));
    }
    if port_driver_key(&hub, port).as_deref() != Some(&key) {
        return Err(unavailable(
            "Selected USB hub port changed during inspection",
        ));
    }
    Ok(Metadata {
        source: "Windows USB hub configuration descriptor for selected Raw Input keyboard".into(),
        speed,
        endpoints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sole_interface_uses_descriptor_number() {
        let descriptor = [
            9, 2, 25, 0, 1, 1, 0, 0x80, 50, 9, 4, 3, 0, 1, 3, 1, 1, 0, 7, 5, 0x81, 3, 8, 0, 1,
        ];
        assert_eq!(selected_interface(&descriptor, None).unwrap(), 3);
        assert_eq!(single_alternate(&descriptor, 3).unwrap(), 0);
    }

    #[test]
    fn multiple_alternates_do_not_pretend_one_is_active() {
        let descriptor = [
            9, 2, 43, 0, 1, 1, 0, 0x80, 50, 9, 4, 3, 0, 1, 3, 1, 1, 0, 7, 5, 0x81, 3, 8, 0, 1, 9,
            4, 3, 1, 1, 3, 1, 1, 0, 7, 5, 0x81, 3, 8, 0, 2,
        ];
        assert!(single_alternate(&descriptor, 3).is_err());
    }
}
