// SPDX-License-Identifier: GPL-3.0-or-later
use super::{InputDevice, InputError, Sample, Sink, spawn};
use byakko_core::model::tester::{Input, InputKind};
use std::{
    io,
    mem::size_of,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::JoinHandle,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::{HANDLE, HWND},
    System::Threading::GetCurrentProcessId,
    UI::{
        Input::KeyboardAndMouse::GetAsyncKeyState,
        Input::{
            GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList,
            GetRegisteredRawInputDevices, RAWINPUT, RAWINPUTDEVICE, RAWINPUTDEVICELIST,
            RAWINPUTHEADER, RAWKEYBOARD, RID_INPUT, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE,
            RIDI_DEVICENAME, RIM_TYPEKEYBOARD, RegisterRawInputDevices,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GIDC_REMOVAL, GetForegroundWindow,
            GetWindowThreadProcessId, HWND_MESSAGE, MSG, MWMO_INPUTAVAILABLE,
            MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT, RI_KEY_BREAK,
            RI_KEY_E0, RI_KEY_E1, TranslateMessage, WM_INPUT, WM_INPUT_DEVICE_CHANGE,
        },
    },
};

pub const TIMESTAMP_SOURCE: &str =
    "Windows Raw Input sampler receipt (monotonic microseconds since start)";

pub fn devices() -> Result<Vec<InputDevice>, InputError> {
    let mut count = 0;
    let size = size_of::<RAWINPUTDEVICELIST>() as u32;
    // SAFETY: null buffer requests the required number of entries.
    let status = unsafe { GetRawInputDeviceList(null_mut(), &mut count, size) };
    if status == u32::MAX {
        return Err(last_error("Could not list keyboard input devices"));
    }
    let mut entries = vec![RAWINPUTDEVICELIST::default(); count as usize];
    // SAFETY: the vector is sized for count entries; count is writable.
    let status = unsafe { GetRawInputDeviceList(entries.as_mut_ptr(), &mut count, size) };
    if status == u32::MAX {
        return Err(last_error("Could not list keyboard input devices"));
    }
    entries.truncate(status as usize);
    let mut found = Vec::new();
    for entry in entries {
        if entry.dwType != RIM_TYPEKEYBOARD {
            continue;
        }
        let Some(id) = device_name(entry.hDevice) else {
            continue;
        };
        let mut parts = id.split('#');
        let hardware = parts.nth(1).unwrap_or("Keyboard");
        let instance = parts.next().unwrap_or("");
        let short_instance: String = instance.chars().take(12).collect();
        found.push(InputDevice {
            label: format!("{hardware} · {short_instance}"),
            identity: id.clone(),
            id,
        });
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(found)
}

fn device_name(handle: HANDLE) -> Option<String> {
    let mut length = 0;
    // SAFETY: null buffer requests UTF-16 length, including terminator.
    if unsafe { GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, null_mut(), &mut length) } != 0
        || length == 0
        || length > 4096
    {
        return None;
    }
    let mut name = vec![0u16; length as usize];
    // SAFETY: buffer is sized in WCHARs as specified by RIDI_DEVICENAME.
    if unsafe {
        GetRawInputDeviceInfoW(
            handle,
            RIDI_DEVICENAME,
            name.as_mut_ptr().cast(),
            &mut length,
        )
    } == u32::MAX
    {
        return None;
    }
    let end = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    String::from_utf16(&name[..end]).ok()
}

fn selected_handle(id: &str) -> Result<HANDLE, InputError> {
    let mut count = 0;
    let size = size_of::<RAWINPUTDEVICELIST>() as u32;
    // SAFETY: sizing query.
    if unsafe { GetRawInputDeviceList(null_mut(), &mut count, size) } == u32::MAX {
        return Err(last_error("Could not inspect selected keyboard"));
    }
    let mut entries = vec![RAWINPUTDEVICELIST::default(); count as usize];
    // SAFETY: vector has the required entry capacity.
    let status = unsafe { GetRawInputDeviceList(entries.as_mut_ptr(), &mut count, size) };
    if status == u32::MAX {
        return Err(last_error("Could not inspect selected keyboard"));
    }
    entries
        .into_iter()
        .take(status as usize)
        .find(|entry| {
            entry.dwType == RIM_TYPEKEYBOARD && device_name(entry.hDevice).as_deref() == Some(id)
        })
        .map(|entry| entry.hDevice)
        .ok_or(InputError::DeviceUnavailable)
}

fn registered_keyboard() -> Result<Option<RAWINPUTDEVICE>, InputError> {
    let mut count = 0;
    let size = size_of::<RAWINPUTDEVICE>() as u32;
    // SAFETY: first call requests registration count for this process.
    let status = unsafe { GetRegisteredRawInputDevices(null_mut(), &mut count, size) };
    if status == u32::MAX {
        return Err(last_error("Could not inspect Raw Input registration"));
    }
    let mut entries = vec![RAWINPUTDEVICE::default(); count as usize];
    // SAFETY: buffer is sized for the registration count.
    let status = unsafe { GetRegisteredRawInputDevices(entries.as_mut_ptr(), &mut count, size) };
    if status == u32::MAX {
        return Err(last_error("Could not inspect Raw Input registration"));
    }
    Ok(entries
        .into_iter()
        .take(status as usize)
        .find(|entry| entry.usUsagePage == 1 && entry.usUsage == 6))
}

fn register(device: RAWINPUTDEVICE) -> Result<(), InputError> {
    // SAFETY: one initialized registration; Win32 copies it synchronously.
    if unsafe { RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) } == 0 {
        Err(last_error("Could not register keyboard Raw Input"))
    } else {
        Ok(())
    }
}

fn last_error(context: &str) -> InputError {
    InputError::Io(format!("{context}: {}", io::Error::last_os_error()))
}

struct Registration {
    window: HWND,
    previous: Option<RAWINPUTDEVICE>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        // The toolkit may re-register on focus change. Never overwrite a newer
        // owner; only restore when our hidden window still owns the keyboard.
        if registered_keyboard()
            .ok()
            .flatten()
            .is_some_and(|entry| entry.hwndTarget == self.window)
        {
            let restore = self.previous.unwrap_or(RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_REMOVE,
                hwndTarget: null_mut(),
            });
            let _ = register(restore);
        }
        // SAFETY: this window was created and is destroyed on its owner thread.
        unsafe { DestroyWindow(self.window) };
    }
}

pub struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(device: &InputDevice, sender: SyncSender<Sample>) -> Result<Self, InputError> {
        if device.id != device.identity {
            return Err(InputError::DeviceUnavailable);
        }
        let selected = selected_handle(&device.id)? as usize;
        let stop = Arc::new(AtomicBool::new(false));
        let cancellation = Arc::clone(&stop);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let thread = spawn("tester-raw-input", move || {
            let initialized = initialize();
            let Ok(registration) = initialized else {
                let _ = ready_sender.send(Err(initialized.err().unwrap()));
                return;
            };
            let _ = ready_sender.send(Ok(()));
            run(selected as HANDLE, sender, cancellation, registration);
        })?;
        ready_receiver
            .recv()
            .map_err(|_| InputError::Io("Raw Input worker stopped during setup".into()))??;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn initialize() -> Result<Registration, InputError> {
    let previous = registered_keyboard()?;
    // Built-in window class, message-only and never visible. It does not
    // suppress legacy keyboard messages to Iced.
    let class: Vec<u16> = "STATIC".encode_utf16().chain(Some(0)).collect();
    // SAFETY: built-in class, null name and process-owned message-only parent.
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            null_mut(),
            null(),
        )
    };
    if window.is_null() {
        return Err(last_error("Could not create Raw Input window"));
    }
    let registration = Registration { window, previous };
    register(RAWINPUTDEVICE {
        usUsagePage: 1,
        usUsage: 6,
        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
        hwndTarget: window,
    })?;
    Ok(registration)
}

fn foreground_is_this_process() -> bool {
    // SAFETY: query the current foreground window's owning process.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(foreground, &mut pid) };
    pid != 0 && pid == unsafe { GetCurrentProcessId() }
}

fn run(
    selected: HANDLE,
    sender: SyncSender<Sample>,
    stop: Arc<AtomicBool>,
    registration: Registration,
) {
    let mut sink = Sink::new(sender);
    let epoch = Instant::now();
    let mut was_foreground = foreground_is_this_process();
    // Snapshot global virtual-key state. A key held before arming cannot be
    // attributed to this keyboard; suppress its makes until the first break.
    let mut held_on_arm = [false; 256];
    for (key, held) in held_on_arm.iter_mut().enumerate() {
        *held = unsafe { GetAsyncKeyState(key as i32) } < 0;
    }
    while !stop.load(Ordering::Acquire) {
        let mut message = MSG::default();
        // SAFETY: owner thread pumps only its own message queue.
        while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_INPUT_DEVICE_CHANGE
                && message.wParam as u32 == GIDC_REMOVAL
                && selected == message.lParam as HANDLE
            {
                sink.send(Sample::Error(InputError::DeviceUnavailable));
                return;
            }
            if message.message == WM_INPUT {
                let foreground = foreground_is_this_process();
                if !foreground {
                    if was_foreground && !sink.send(Sample::Discontinuity) {
                        return;
                    }
                    was_foreground = false;
                } else {
                    if !was_foreground && !sink.send(Sample::Discontinuity) {
                        return;
                    }
                    was_foreground = true;
                    let at_us = epoch.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
                    if let Some((input, vkey)) = read_input(message.lParam, selected, at_us) {
                        let suppress = held_on_arm.get_mut(vkey as usize).is_some_and(|held| {
                            if !*held {
                                return false;
                            }
                            if input.kind == InputKind::Release {
                                *held = false;
                            }
                            true
                        });
                        if !suppress && !sink.send(Sample::Input(input)) {
                            return;
                        }
                    }
                }
            }
            // SAFETY: default processing frees foreground raw input handles.
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message)
            };
        }
        if !sink.flush_gap() {
            break;
        }
        match registered_keyboard() {
            Ok(Some(current)) if current.hwndTarget == registration.window => {}
            Ok(_) => {
                sink.send(Sample::Error(InputError::RegistrationConflict));
                break;
            }
            Err(error) => {
                sink.send(Sample::Error(error));
                break;
            }
        }
        // Wake as soon as input arrives; the timeout also bounds Drop latency.
        unsafe { MsgWaitForMultipleObjectsEx(0, null(), 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
    }
}

fn read_input(raw_handle: isize, selected: HANDLE, at_us: u64) -> Option<(Input, u16)> {
    let mut size = size_of::<RAWINPUT>() as u32;
    let mut data = RAWINPUT::default();
    // Foreground PID was checked before decoding or sending.
    // SAFETY: buffer and size are writable; RAWINPUT includes keyboard payload.
    let status = unsafe {
        GetRawInputData(
            raw_handle as *mut _,
            RID_INPUT,
            (&mut data as *mut RAWINPUT).cast(),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if status == u32::MAX
        || status < (size_of::<RAWINPUTHEADER>() + size_of::<RAWKEYBOARD>()) as u32
        || data.header.dwSize < (size_of::<RAWINPUTHEADER>() + size_of::<RAWKEYBOARD>()) as u32
        || data.header.dwSize > status
        || data.header.dwType != RIM_TYPEKEYBOARD
        || data.header.hDevice != selected
    {
        return None;
    }
    // SAFETY: dwType checked above, so keyboard is the active union member.
    let keyboard = unsafe { data.data.keyboard };
    let usage = hid_usage(keyboard)?;
    let kind = if u32::from(keyboard.Flags) & RI_KEY_BREAK != 0 {
        InputKind::Release
    } else {
        InputKind::Press
    };
    Some((Input { usage, kind, at_us }, keyboard.VKey))
}

// Set 1 make code with E0/E1 extension to HID Keyboard/Keypad usage. The
// extension distinguishes navigation from keypad and left/right modifiers.
fn hid_usage(key: RAWKEYBOARD) -> Option<u16> {
    let flags = u32::from(key.Flags);
    let e0 = flags & RI_KEY_E0 != 0;
    let e1 = flags & RI_KEY_E1 != 0;
    let scan = key.MakeCode;
    if e1 {
        return (scan == 0x45).then_some(0x48);
    }
    if e0 {
        return Some(match scan {
            0x1c => 0x58,
            0x1d => 0xe4,
            0x35 => 0x54,
            0x37 => 0x46,
            0x38 => 0xe6,
            0x47 => 0x4a,
            0x48 => 0x52,
            0x49 => 0x4b,
            0x4b => 0x50,
            0x4d => 0x4f,
            0x4f => 0x4d,
            0x50 => 0x51,
            0x51 => 0x4e,
            0x52 => 0x49,
            0x53 => 0x4c,
            0x5b => 0xe3,
            0x5c => 0xe7,
            0x5d => 0x65,
            _ => return None,
        });
    }
    Some(match scan {
        0x01 => 0x29,
        0x02 => 0x1e,
        0x03 => 0x1f,
        0x04 => 0x20,
        0x05 => 0x21,
        0x06 => 0x22,
        0x07 => 0x23,
        0x08 => 0x24,
        0x09 => 0x25,
        0x0a => 0x26,
        0x0b => 0x27,
        0x0c => 0x2d,
        0x0d => 0x2e,
        0x0e => 0x2a,
        0x0f => 0x2b,
        0x10 => 0x14,
        0x11 => 0x1a,
        0x12 => 0x08,
        0x13 => 0x15,
        0x14 => 0x17,
        0x15 => 0x1c,
        0x16 => 0x18,
        0x17 => 0x0c,
        0x18 => 0x12,
        0x19 => 0x13,
        0x1a => 0x2f,
        0x1b => 0x30,
        0x1c => 0x28,
        0x1d => 0xe0,
        0x1e => 0x04,
        0x1f => 0x16,
        0x20 => 0x07,
        0x21 => 0x09,
        0x22 => 0x0a,
        0x23 => 0x0b,
        0x24 => 0x0d,
        0x25 => 0x0e,
        0x26 => 0x0f,
        0x27 => 0x33,
        0x28 => 0x34,
        0x29 => 0x35,
        0x2a => 0xe1,
        0x2b => 0x31,
        0x2c => 0x1d,
        0x2d => 0x1b,
        0x2e => 0x06,
        0x2f => 0x19,
        0x30 => 0x05,
        0x31 => 0x11,
        0x32 => 0x10,
        0x33 => 0x36,
        0x34 => 0x37,
        0x35 => 0x38,
        0x36 => 0xe5,
        0x37 => 0x55,
        0x38 => 0xe2,
        0x39 => 0x2c,
        0x3a => 0x39,
        0x3b => 0x3a,
        0x3c => 0x3b,
        0x3d => 0x3c,
        0x3e => 0x3d,
        0x3f => 0x3e,
        0x40 => 0x3f,
        0x41 => 0x40,
        0x42 => 0x41,
        0x43 => 0x42,
        0x44 => 0x43,
        0x45 => 0x53,
        0x46 => 0x47,
        0x47 => 0x5f,
        0x48 => 0x60,
        0x49 => 0x61,
        0x4a => 0x56,
        0x4b => 0x5c,
        0x4c => 0x5d,
        0x4d => 0x5e,
        0x4e => 0x57,
        0x4f => 0x59,
        0x50 => 0x5a,
        0x51 => 0x5b,
        0x52 => 0x62,
        0x53 => 0x63,
        0x56 => 0x64,
        0x57 => 0x44,
        0x58 => 0x45,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_extensions_keep_keypad_and_sides_distinct() {
        let key = RAWKEYBOARD {
            MakeCode: 0x1c,
            ..Default::default()
        };
        assert_eq!(hid_usage(key), Some(0x28));
        assert_eq!(
            hid_usage(RAWKEYBOARD {
                Flags: RI_KEY_E0 as u16,
                ..key
            }),
            Some(0x58)
        );
        let left = RAWKEYBOARD {
            MakeCode: 0x1d,
            ..Default::default()
        };
        assert_eq!(hid_usage(left), Some(0xe0));
        assert_eq!(
            hid_usage(RAWKEYBOARD {
                Flags: RI_KEY_E0 as u16,
                ..left
            }),
            Some(0xe4)
        );
        assert_eq!(
            hid_usage(RAWKEYBOARD {
                MakeCode: 0x2a,
                Flags: RI_KEY_E0 as u16,
                ..Default::default()
            }),
            None
        );
    }
}
