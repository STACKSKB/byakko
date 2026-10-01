use std::{
    ffi::{CStr, CString, c_char, c_void},
    ptr,
    time::Duration,
};

use super::{DeviceInfo, Result, descriptor::parse_descriptor};

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFSetRef = *const c_void;
type CFAllocatorRef = *const c_void;
type IoService = u32;
type IoReturn = i32;
type IoHidDevice = *mut c_void;
type IoHidManager = *mut c_void;

const HOST_REPORT_LEN: usize = 65;
const KERN_SUCCESS: IoReturn = 0;
const FEATURE_REPORT: u32 = 2;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOHIDManagerCreate(allocator: CFAllocatorRef, options: u32) -> IoHidManager;
    fn IOHIDManagerSetDeviceMatching(manager: IoHidManager, matching: CFTypeRef);
    fn IOHIDManagerOpen(manager: IoHidManager, options: u32) -> IoReturn;
    fn IOHIDManagerClose(manager: IoHidManager, options: u32) -> IoReturn;
    fn IOHIDManagerCopyDevices(manager: IoHidManager) -> CFSetRef;
    fn IOHIDDeviceGetProperty(device: IoHidDevice, key: CFStringRef) -> CFTypeRef;
    fn IOHIDDeviceGetService(device: IoHidDevice) -> IoService;
    fn IOHIDDeviceOpen(device: IoHidDevice, options: u32) -> IoReturn;
    fn IOHIDDeviceClose(device: IoHidDevice, options: u32) -> IoReturn;
    fn IOHIDDeviceSetReport(
        device: IoHidDevice,
        report_type: u32,
        report_id: isize,
        report: *const u8,
        report_length: isize,
    ) -> IoReturn;
    fn IOHIDDeviceGetReport(
        device: IoHidDevice,
        report_type: u32,
        report_id: isize,
        report: *mut u8,
        report_length: *mut isize,
    ) -> IoReturn;
    fn IOHIDDeviceScheduleWithRunLoop(device: IoHidDevice, run_loop: CFTypeRef, mode: CFStringRef);
    fn IOHIDDeviceUnscheduleFromRunLoop(
        device: IoHidDevice,
        run_loop: CFTypeRef,
        mode: CFStringRef,
    );
    fn IOHIDDeviceRegisterInputReportCallback(
        device: IoHidDevice,
        report: *mut u8,
        report_length: isize,
        callback: Option<
            unsafe extern "C" fn(*mut c_void, IoReturn, *mut c_void, u32, u32, *mut u8, isize),
        >,
        context: *mut c_void,
    );
    fn IORegistryEntryGetParentEntry(
        entry: IoService,
        plane: *const c_char,
        parent: *mut IoService,
    ) -> IoReturn;
    fn IORegistryEntryGetRegistryEntryID(entry: IoService, registry_id: *mut u64) -> IoReturn;
    fn IORegistryEntryCreateCFProperty(
        entry: IoService,
        key: CFStringRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> CFTypeRef;
    fn IOObjectRetain(object: IoService) -> IoReturn;
    fn IOObjectRelease(object: IoService) -> IoReturn;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFAllocatorGetDefault() -> CFAllocatorRef;
    fn CFStringCreateWithCString(
        allocator: CFAllocatorRef,
        value: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        value: CFStringRef,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
    fn CFNumberGetValue(number: CFTypeRef, number_type: i32, value: *mut i32) -> bool;
    fn CFDataGetLength(data: CFTypeRef) -> isize;
    fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
    fn CFSetGetCount(set: CFSetRef) -> isize;
    fn CFSetGetValues(set: CFSetRef, values: *mut CFTypeRef);
    fn CFRetain(value: CFTypeRef) -> CFTypeRef;
    fn CFRelease(value: CFTypeRef);
    fn CFRunLoopGetCurrent() -> CFTypeRef;
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source: bool) -> i32;
    static kCFRunLoopDefaultMode: CFStringRef;
}

struct Manager(IoHidManager);

impl Manager {
    fn open() -> Result<Self> {
        let manager = unsafe { IOHIDManagerCreate(CFAllocatorGetDefault(), 0) };
        if manager.is_null() {
            return Err("Could not create macOS HID manager".into());
        }
        unsafe { IOHIDManagerSetDeviceMatching(manager, ptr::null()) };
        if unsafe { IOHIDManagerOpen(manager, 0) } != KERN_SUCCESS {
            unsafe { CFRelease(manager) };
            return Err("Could not open macOS HID manager".into());
        }
        Ok(Self(manager))
    }

    fn devices(&self) -> Result<Vec<CFTypeRef>> {
        let set = unsafe { IOHIDManagerCopyDevices(self.0) };
        if set.is_null() {
            return Err("Could not enumerate macOS HID devices".into());
        }
        let count = unsafe { CFSetGetCount(set) }.max(0) as usize;
        let mut devices = vec![ptr::null(); count];
        if count > 0 {
            unsafe { CFSetGetValues(set, devices.as_mut_ptr()) };
        }
        unsafe { CFRelease(set) };
        Ok(devices)
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        unsafe {
            IOHIDManagerClose(self.0, 0);
            CFRelease(self.0);
        }
    }
}

fn cf_key(name: &str) -> Option<CFStringRef> {
    let name = CString::new(name).ok()?;
    let key =
        unsafe { CFStringCreateWithCString(CFAllocatorGetDefault(), name.as_ptr(), 0x08000100) };
    (!key.is_null()).then_some(key)
}

fn property(device: IoHidDevice, name: &str) -> CFTypeRef {
    let Some(key) = cf_key(name) else {
        return ptr::null();
    };
    let value = unsafe { IOHIDDeviceGetProperty(device, key) };
    unsafe { CFRelease(key) };
    value
}

fn number_property(device: IoHidDevice, name: &str) -> Option<u16> {
    let value = property(device, name);
    if value.is_null() {
        return None;
    }
    let mut number = 0i32;
    let valid = unsafe { CFNumberGetValue(value, 3, &mut number) };
    (valid && (0..=u16::MAX as i32).contains(&number)).then_some(number as u16)
}

fn data_property(device: IoHidDevice, name: &str) -> Option<Vec<u8>> {
    let data = property(device, name);
    if data.is_null() {
        return None;
    }
    let length = unsafe { CFDataGetLength(data) };
    let bytes = unsafe { CFDataGetBytePtr(data) };
    if length <= 0 || length > 4096 || bytes.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec())
}

fn text_property(device: IoHidDevice, name: &str) -> Option<String> {
    let value = property(device, name);
    if value.is_null() {
        return None;
    }
    let mut buffer = [0i8; 1024];
    let valid = unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            0x08000100,
        )
    };
    if !valid {
        return None;
    }
    let text = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    (!text.is_empty()).then_some(text)
}

fn registry_number(entry: IoService, name: &str) -> Option<u16> {
    let key = cf_key(name)?;
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, key, CFAllocatorGetDefault(), 0) };
    unsafe { CFRelease(key) };
    if value.is_null() {
        return None;
    }
    let mut number = 0i32;
    let valid = unsafe { CFNumberGetValue(value, 3, &mut number) };
    unsafe { CFRelease(value) };
    (valid && (0..=u16::MAX as i32).contains(&number)).then_some(number as u16)
}

fn registry_ancestor_number(mut entry: IoService, name: &str) -> Option<u16> {
    unsafe { IOObjectRetain(entry) };
    let plane = c"IOService";
    for _ in 0..32 {
        if let Some(number) = registry_number(entry, name) {
            unsafe { IOObjectRelease(entry) };
            return Some(number);
        }
        let mut parent = 0;
        if unsafe { IORegistryEntryGetParentEntry(entry, plane.as_ptr(), &mut parent) }
            != KERN_SUCCESS
        {
            unsafe { IOObjectRelease(entry) };
            return None;
        }
        unsafe { IOObjectRelease(entry) };
        entry = parent;
    }
    unsafe { IOObjectRelease(entry) };
    None
}

fn physical_device(mut entry: IoService) -> Option<String> {
    unsafe { IOObjectRetain(entry) };
    let plane = c"IOService";
    for _ in 0..32 {
        if registry_number(entry, "idVendor") == Some(0x3151)
            && matches!(registry_number(entry, "idProduct"), Some(0x4011 | 0x4015))
        {
            let mut id = 0;
            let result = unsafe { IORegistryEntryGetRegistryEntryID(entry, &mut id) };
            unsafe { IOObjectRelease(entry) };
            return (result == KERN_SUCCESS).then(|| id.to_string());
        }
        let mut parent = 0;
        if unsafe { IORegistryEntryGetParentEntry(entry, plane.as_ptr(), &mut parent) }
            != KERN_SUCCESS
        {
            unsafe { IOObjectRelease(entry) };
            return None;
        }
        unsafe { IOObjectRelease(entry) };
        entry = parent;
    }
    unsafe { IOObjectRelease(entry) };
    None
}

fn describe(device: IoHidDevice) -> Option<DeviceInfo> {
    let vid = number_property(device, "VendorID")?;
    let pid = number_property(device, "ProductID")?;
    if vid != 0x3151 || !matches!(pid, 0x4011 | 0x4015) {
        return None;
    }
    let descriptor = data_property(device, "ReportDescriptor")?;
    let parsed = parse_descriptor(&descriptor)?;
    let usage = parsed.application_usage?;
    let service = unsafe { IOHIDDeviceGetService(device) };
    if service == 0 {
        return None;
    }
    let mut registry_id = 0;
    if unsafe { IORegistryEntryGetRegistryEntryID(service, &mut registry_id) } != KERN_SUCCESS {
        return None;
    }
    Some(DeviceInfo {
        physical_device: physical_device(service),
        path: CString::new(format!("IOHID:{registry_id}")).ok()?,
        vid,
        pid,
        interface: registry_ancestor_number(service, "bInterfaceNumber").map_or(-1, i32::from),
        usage_page: usage.0,
        usage: usage.1,
        manufacturer: text_property(device, "Manufacturer"),
        product: text_property(device, "Product"),
        release: number_property(device, "VersionNumber").unwrap_or(0),
    })
}

pub struct Device {
    device: IoHidDevice,
    _manager: Manager,
}

unsafe impl Send for Device {}

impl Device {
    pub fn send_feature_report(&self, report: &[u8]) -> Result<()> {
        if report.len() != HOST_REPORT_LEN || report[0] != 0 {
            return Err("Nia87 feature report must be 65 bytes with report ID zero".into());
        }
        let result = unsafe {
            IOHIDDeviceSetReport(
                self.device,
                FEATURE_REPORT,
                0,
                report[1..].as_ptr(),
                (report.len() - 1) as isize,
            )
        };
        if result != KERN_SUCCESS {
            return Err(format!("Could not send macOS HID feature report ({result:#x})").into());
        }
        Ok(())
    }

    pub fn get_feature_report(&self, report: &mut [u8]) -> Result<usize> {
        if report.len() != HOST_REPORT_LEN || report[0] != 0 {
            return Err("Nia87 feature report buffer must be 65 bytes with report ID zero".into());
        }
        let mut length = (report.len() - 1) as isize;
        let result = unsafe {
            IOHIDDeviceGetReport(
                self.device,
                FEATURE_REPORT,
                0,
                report[1..].as_mut_ptr(),
                &mut length,
            )
        };
        if result != KERN_SUCCESS || length != (report.len() - 1) as isize {
            return Err(format!(
                "Could not read a complete macOS HID feature report ({result:#x})"
            )
            .into());
        }
        Ok(report.len())
    }

    pub fn get_report_descriptor(&self, output: &mut [u8]) -> Result<usize> {
        let descriptor = data_property(self.device, "ReportDescriptor")
            .ok_or("macOS did not provide the HID report descriptor")?;
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

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            IOHIDDeviceClose(self.device, 0);
            CFRelease(self.device);
        }
    }
}

pub fn enumerate() -> Result<Vec<DeviceInfo>> {
    let manager = Manager::open()?;
    let mut found = manager
        .devices()?
        .into_iter()
        .filter_map(|device| describe(device as IoHidDevice))
        .collect::<Vec<_>>();
    found.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(found)
}

pub fn open(path: &CStr) -> Result<Device> {
    let inventory = Manager::open()?;
    let wanted = path.to_bytes();
    for candidate in inventory.devices()? {
        let candidate = candidate as IoHidDevice;
        let Some(info) = describe(candidate) else {
            continue;
        };
        if info.path.as_bytes() != wanted {
            continue;
        }
        let parsed = data_property(candidate, "ReportDescriptor")
            .and_then(|bytes| parse_descriptor(&bytes))
            .ok_or("Malformed Nia87 HID report descriptor")?;
        if info.usage_page != 0xffff
            || info.usage != 2
            || !parsed.target
            || parsed.target_feature_count != 1
            || !parsed.target_feature_shape_valid
        {
            return Err("Nia87 collection must have one unnumbered 64-byte feature report".into());
        }
        unsafe { CFRetain(candidate) };
        if unsafe { IOHIDDeviceOpen(candidate, 0) } != KERN_SUCCESS {
            unsafe { CFRelease(candidate) };
            return Err("Could not open selected macOS Nia87 HID collection".into());
        }
        return Ok(Device {
            device: candidate,
            _manager: inventory,
        });
    }
    Err("Selected macOS HID collection is disconnected".into())
}

pub(crate) struct InputDevice {
    device: IoHidDevice,
    _manager: Manager,
    report: Box<[u8; 4]>,
    received: Box<CallbackState>,
    scheduled: bool,
}

unsafe impl Send for InputDevice {}

struct CallbackState {
    report: Option<[u8; 4]>,
    failed: bool,
}

unsafe extern "C" fn input_report(
    context: *mut c_void,
    result: IoReturn,
    _: *mut c_void,
    _: u32,
    report_id: u32,
    report: *mut u8,
    length: isize,
) {
    if context.is_null() || report_id != 5 {
        return;
    }
    let state = unsafe { &mut *(context.cast::<CallbackState>()) };
    if result != KERN_SUCCESS || report.is_null() {
        state.failed = true;
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(report, length.max(0) as usize) };
    state.report = match bytes {
        [first, second, third] => Some([5, *first, *second, *third]),
        [5, second, third, fourth] => Some([5, *second, *third, *fourth]),
        _ => {
            state.failed = true;
            None
        }
    };
}

impl InputDevice {
    pub(crate) fn open(path: &CStr) -> Result<Self> {
        let inventory = Manager::open()?;
        let wanted = path.to_bytes();
        for candidate in inventory.devices()? {
            let candidate = candidate as IoHidDevice;
            let Some(info) = describe(candidate) else {
                continue;
            };
            if info.path.as_bytes() != wanted || info.usage_page != 0xffff || info.usage != 1 {
                continue;
            }
            let descriptor = data_property(candidate, "ReportDescriptor")
                .and_then(|bytes| parse_descriptor(&bytes))
                .ok_or("Malformed Nia87 notification report descriptor")?;
            if !descriptor.notification_input_valid {
                return Err(
                    "Keyboard notifications require report ID 5 with a 3-byte payload".into(),
                );
            }
            unsafe { CFRetain(candidate) };
            if unsafe { IOHIDDeviceOpen(candidate, 0) } != KERN_SUCCESS {
                unsafe { CFRelease(candidate) };
                return Err("Could not open macOS keyboard notification collection".into());
            }
            let mut input = Self {
                device: candidate,
                _manager: inventory,
                report: Box::new([0; 4]),
                received: Box::new(CallbackState {
                    report: None,
                    failed: false,
                }),
                scheduled: false,
            };
            let run_loop = unsafe { CFRunLoopGetCurrent() };
            unsafe {
                IOHIDDeviceRegisterInputReportCallback(
                    candidate,
                    input.report.as_mut_ptr(),
                    input.report.len() as isize,
                    Some(input_report),
                    (&mut *input.received as *mut CallbackState).cast(),
                );
                IOHIDDeviceScheduleWithRunLoop(candidate, run_loop, kCFRunLoopDefaultMode);
            }
            input.scheduled = true;
            return Ok(input);
        }
        Err("Selected macOS keyboard notification collection is disconnected".into())
    }

    pub(crate) fn read_timeout(&mut self, timeout: Duration) -> Result<Option<[u8; 4]>> {
        if let Some(report) = self.received.report.take() {
            return Ok(Some(report));
        }
        let result = unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, timeout.as_secs_f64().min(0.1), true)
        };
        if result == 1 || result == 2 {
            return Err("macOS keyboard notification run loop stopped".into());
        }
        if self.received.failed {
            return Err("macOS keyboard notification report was invalid".into());
        }
        Ok(self.received.report.take())
    }
}

impl Drop for InputDevice {
    fn drop(&mut self) {
        unsafe {
            if self.scheduled {
                IOHIDDeviceUnscheduleFromRunLoop(
                    self.device,
                    CFRunLoopGetCurrent(),
                    kCFRunLoopDefaultMode,
                );
            }
            IOHIDDeviceClose(self.device, 0);
            CFRelease(self.device);
        }
    }
}
