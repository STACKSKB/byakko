//! Read-only, overlapped input on the sibling vendor-event collection.

use super::{Device, Preparsed, Result, attributes, capabilities, error, wide_path};
use std::{
    ffi::CStr,
    io,
    mem::size_of,
    ptr::{null, null_mut},
    time::Duration,
};
use windows_sys::Win32::{
    Devices::HumanInterfaceDevice::{
        HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidD_GetPreparsedData, HidP_GetValueCaps, HidP_Input,
    },
    Foundation::{
        CloseHandle, ERROR_IO_PENDING, GENERIC_READ, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        ReadFile,
    },
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        Threading::{CreateEventW, ResetEvent, WaitForSingleObject},
    },
};

struct PendingRead {
    report: [u8; 4],
    overlapped: OVERLAPPED,
}

pub(crate) struct InputDevice {
    device: Device,
    // Both pointers supplied to ReadFile remain stable until completion.
    pending: Box<PendingRead>,
    active: bool,
}

// The owner moves the listener to one worker; there are no shared calls.
unsafe impl Send for InputDevice {}

impl InputDevice {
    pub(crate) fn open(path: &CStr) -> Result<Self> {
        let path = wide_path(path)?;
        // SAFETY: zero-terminated path and documented flags. No write access.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(error("Could not open keyboard notifications"));
        }
        let device = Device { handle };
        let attr = attributes(handle)?;
        let caps = capabilities(handle)?;
        if attr.VendorID != 0x3151
            || !matches!(attr.ProductID, 0x4011 | 0x4015)
            || caps.UsagePage != 0xffff
            || caps.Usage != 1
            || caps.InputReportByteLength != 4
        {
            return Err("Keyboard notification collection has an incompatible input report".into());
        }
        let mut pointer = 0;
        // SAFETY: writable pointer; guard releases the returned preparsed data.
        if !unsafe { HidD_GetPreparsedData(handle, &mut pointer) } {
            return Err(error("Could not inspect keyboard input reports"));
        }
        let preparsed = Preparsed(pointer);
        let mut values = vec![HIDP_VALUE_CAPS::default(); caps.NumberInputValueCaps as usize];
        let mut count = values.len() as u16;
        // SAFETY: allocation matches the capability count and preparsed is live.
        let status =
            unsafe { HidP_GetValueCaps(HidP_Input, values.as_mut_ptr(), &mut count, preparsed.0) };
        if status != HIDP_STATUS_SUCCESS
            || count != 1
            || values[0].ReportID != 5
            || values[0].BitSize != 8
            || values[0].ReportCount != 3
        {
            return Err("Keyboard notifications require report ID 5 with a 3-byte payload".into());
        }
        // SAFETY: unnamed manual-reset event, owned and closed in Drop.
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() {
            return Err(error("Could not create keyboard input event"));
        }
        Ok(Self {
            device,
            pending: Box::new(PendingRead {
                report: [0; 4],
                overlapped: OVERLAPPED {
                    hEvent: event,
                    ..Default::default()
                },
            }),
            active: false,
        })
    }

    pub(crate) fn read_timeout(&mut self, timeout: Duration) -> Result<Option<[u8; 4]>> {
        if !self.active {
            let event = self.pending.overlapped.hEvent;
            self.pending.overlapped = OVERLAPPED {
                hEvent: event,
                ..Default::default()
            };
            // SAFETY: the event and buffers are owned and remain stable in the box.
            if unsafe { ResetEvent(event) } == 0 {
                return Err(error("Could not reset keyboard input event"));
            }
            self.pending.report.fill(0);
            let started = unsafe {
                ReadFile(
                    self.device.handle,
                    self.pending.report.as_mut_ptr(),
                    size_of::<[u8; 4]>() as u32,
                    null_mut(),
                    &mut self.pending.overlapped,
                )
            };
            if started == 0
                && io::Error::last_os_error().raw_os_error() != Some(ERROR_IO_PENDING as i32)
            {
                return Err(error("Could not receive keyboard notification"));
            }
            self.active = true;
        }
        // A timeout keeps the same outstanding read: it sends no USB request.
        let timeout = timeout.as_millis().min(100) as u32;
        // SAFETY: event remains valid until this owner is dropped.
        match unsafe { WaitForSingleObject(self.pending.overlapped.hEvent, timeout) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut count = 0;
                // SAFETY: buffers remain live; signaled event means completed I/O.
                let success = unsafe {
                    GetOverlappedResult(self.device.handle, &self.pending.overlapped, &mut count, 0)
                };
                self.active = false;
                if success == 0 {
                    return Err(error("Keyboard notification read failed"));
                }
                if count != 4 {
                    return Err("Keyboard notification report was incomplete".into());
                }
                Ok(Some(self.pending.report))
            }
            _ => Err(error("Could not wait for keyboard notification")),
        }
    }
}

impl Drop for InputDevice {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: cancel only our outstanding operation, then drain it before
            // freeing either buffer. CancelIoEx alone does not finish the I/O.
            unsafe {
                CancelIoEx(self.device.handle, &self.pending.overlapped);
                let mut count = 0;
                GetOverlappedResult(self.device.handle, &self.pending.overlapped, &mut count, 1);
            }
        }
        // SAFETY: owned event; no outstanding operation can reference it now.
        unsafe { CloseHandle(self.pending.overlapped.hEvent) };
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Opt-in, read-only collection verification. Never emits HID reports.
    #[test]
    #[ignore = "requires the connected USB Nia87; read-only hardware evidence"]
    fn inspect_live_notification_collection() {
        let inventory = crate::hid::HidApi::new().unwrap();
        for info in inventory
            .device_list()
            .filter(|info| info.vendor_id() == 0x3151)
        {
            let device = super::super::open_handle(info.path(), 0).unwrap();
            let caps = capabilities(device.handle).unwrap();
            println!(
                "collection={:?} parent={:?} input={} output={} feature={} value_caps={}",
                info.identity(),
                info.physical_device,
                caps.InputReportByteLength,
                caps.OutputReportByteLength,
                caps.FeatureReportByteLength,
                caps.NumberInputValueCaps
            );
            if info.usage_page() == 0xffff && info.usage() == 1 {
                let mut listener = InputDevice::open(info.path()).unwrap();
                println!("verified input shape: report ID 5, 8-bit fields, count 3");
                println!(
                    "idle read: {:?}",
                    listener.read_timeout(Duration::from_millis(100)).unwrap()
                );
                // Drop must cancel and drain this idle pending read promptly.
            }
        }
    }
}
