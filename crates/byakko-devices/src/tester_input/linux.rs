// SPDX-License-Identifier: GPL-3.0-or-later
use super::{InputDevice, InputError, Sample, Sink, spawn};
use byakko_core::model::tester::{Input, InputKind};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    thread::JoinHandle,
};

pub const TIMESTAMP_SOURCE: &str = "evdev kernel monotonic timestamp (microseconds)";
const EV_KEY: u16 = 1;
const EV_SYN: u16 = 0;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
// Linux input.h: _IOW('E', 0xa0, int). A failed request is explicit because
// default evdev timestamps may use a different clock.
const EVIOCSCLOCKID: libc::c_ulong = 0x4004_45a0;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Event {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

pub fn devices() -> Result<Vec<InputDevice>, InputError> {
    let mut found = Vec::new();
    for entry in fs::read_dir("/sys/class/input").map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !valid_event_name(name) {
            continue;
        }
        let base = entry.path().join("device");
        // A keyboard must expose EV_KEY and at least an ordinary keyboard key.
        let keys = fs::read_to_string(base.join("capabilities/key")).unwrap_or_default();
        if !bitmap_has(&keys, 30) || !bitmap_has(&keys, 28) {
            continue;
        }
        let label = fs::read_to_string(base.join("name"))
            .unwrap_or_else(|_| "Keyboard".into())
            .trim()
            .to_owned();
        let Ok(identity) = fs::canonicalize(entry.path()) else {
            continue;
        };
        found.push(InputDevice {
            id: format!("/dev/input/{name}"),
            identity: identity.to_string_lossy().into_owned(),
            label: format!("{label} ({name})"),
        });
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(found)
}

fn valid_event_name(name: &str) -> bool {
    name.strip_prefix("event").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// /sys capability bitmaps print the highest word first; each word is native
/// unsigned long, so use the machine width rather than assuming 32 bits.
fn bitmap_has(text: &str, bit: usize) -> bool {
    let words: Vec<_> = text.split_whitespace().collect();
    let word_bits = usize::BITS as usize;
    let index_from_right = bit / word_bits;
    let Some(word) = words.iter().rev().nth(index_from_right) else {
        return false;
    };
    usize::from_str_radix(word, 16).is_ok_and(|value| value & (1usize << (bit % word_bits)) != 0)
}

pub struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(device: &InputDevice, sender: SyncSender<Sample>) -> Result<Self, InputError> {
        let path = Path::new(&device.id);
        if path.parent() != Some(Path::new("/dev/input"))
            || !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(valid_event_name)
        {
            return Err(InputError::DeviceUnavailable);
        }
        let class = Path::new("/sys/class/input").join(path.file_name().unwrap());
        if fs::canonicalize(&class)
            .map_err(|_| InputError::DeviceUnavailable)?
            .to_string_lossy()
            != device.identity
        {
            return Err(InputError::DeviceUnavailable);
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .map_err(io_error)?;
        // Recheck after open; eventX may have been reused during Start.
        if fs::canonicalize(&class)
            .map_err(|_| InputError::DeviceUnavailable)?
            .to_string_lossy()
            != device.identity
        {
            return Err(InputError::DeviceUnavailable);
        }
        let numbers =
            fs::read_to_string(class.join("dev")).map_err(|_| InputError::DeviceUnavailable)?;
        let Some((major, minor)) = numbers.trim().split_once(':') else {
            return Err(InputError::DeviceUnavailable);
        };
        let major = major
            .parse::<u32>()
            .map_err(|_| InputError::DeviceUnavailable)?;
        let minor = minor
            .parse::<u32>()
            .map_err(|_| InputError::DeviceUnavailable)?;
        let opened = file.metadata().map_err(io_error)?.rdev();
        if libc::major(opened) != major || libc::minor(opened) != minor {
            return Err(InputError::DeviceUnavailable);
        }
        let clock = libc::CLOCK_MONOTONIC;
        // SAFETY: live evdev fd and a readable int for the synchronous ioctl.
        if unsafe { libc::ioctl(file.as_raw_fd(), EVIOCSCLOCKID, &clock) } < 0 {
            return Err(io_error(io::Error::last_os_error()));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let cancellation = Arc::clone(&stop);
        let thread = spawn("tester-evdev", move || run(file, sender, cancellation))?;
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

fn run(mut file: File, sender: SyncSender<Sample>, stop: Arc<AtomicBool>) {
    let mut sink = Sink::new(sender);
    let mut dropped = false;
    while !stop.load(Ordering::Acquire) {
        let mut poll = libc::pollfd {
            fd: file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: valid pollfd, one descriptor and a bounded cancellation wait.
        let status = unsafe { libc::poll(&mut poll, 1, 50) };
        if status < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            sink.send(Sample::Error(io_error(io::Error::last_os_error())));
            break;
        }
        if status == 0 {
            if !sink.flush_gap() {
                break;
            }
            continue;
        }
        if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            sink.send(Sample::Error(InputError::DeviceUnavailable));
            break;
        }
        let mut raw = [0u8; std::mem::size_of::<Event>()];
        match file.read(&mut raw) {
            Ok(count) if count == raw.len() => {
                // SAFETY: Event is a plain C struct; read_unaligned avoids relying
                // on byte-buffer alignment. All bytes were filled by evdev.
                let event = unsafe { std::ptr::read_unaligned(raw.as_ptr().cast::<Event>()) };
                if event.kind == EV_SYN && event.code == SYN_DROPPED {
                    dropped = true;
                    if !sink.send(Sample::Discontinuity) {
                        break;
                    }
                } else if dropped {
                    if event.kind == EV_SYN && event.code == SYN_REPORT {
                        dropped = false
                    }
                } else if let Some(input) = decode(event) {
                    if !sink.send(Sample::Input(input)) {
                        break;
                    }
                }
            }
            Ok(0) => {
                sink.send(Sample::Error(InputError::DeviceUnavailable));
                break;
            }
            Ok(_) => {
                sink.send(Sample::Error(InputError::Io(
                    "Incomplete evdev event".into(),
                )));
                break;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => {
                sink.send(Sample::Error(io_error(error)));
                break;
            }
        }
    }
}

fn io_error(error: io::Error) -> InputError {
    if error.kind() == io::ErrorKind::PermissionDenied {
        InputError::PermissionDenied
    } else {
        InputError::Io(format!("Keyboard input: {error}"))
    }
}

fn decode(event: Event) -> Option<Input> {
    if event.kind != EV_KEY {
        return None;
    }
    let kind = match event.value {
        0 => InputKind::Release,
        1 => InputKind::Press,
        2 => InputKind::Repeat,
        _ => return None,
    };
    let usage = hid_usage(event.code)?;
    let seconds = u64::try_from(event.time.tv_sec).ok()?;
    let micros = u64::try_from(event.time.tv_usec).ok()?;
    if micros >= 1_000_000 {
        return None;
    }
    Some(Input {
        usage,
        kind,
        at_us: seconds.saturating_mul(1_000_000).saturating_add(micros),
    })
}

// Linux input-event-codes.h EV_KEY to USB HID Keyboard/Keypad usage page.
// Unknown consumer/system keys are skipped rather than guessed.
fn hid_usage(code: u16) -> Option<u16> {
    Some(match code {
        1 => 0x29,
        2 => 0x1e,
        3 => 0x1f,
        4 => 0x20,
        5 => 0x21,
        6 => 0x22,
        7 => 0x23,
        8 => 0x24,
        9 => 0x25,
        10 => 0x26,
        11 => 0x27,
        12 => 0x2d,
        13 => 0x2e,
        14 => 0x2a,
        15 => 0x2b,
        16 => 0x14,
        17 => 0x1a,
        18 => 0x08,
        19 => 0x15,
        20 => 0x17,
        21 => 0x1c,
        22 => 0x18,
        23 => 0x0c,
        24 => 0x12,
        25 => 0x13,
        26 => 0x2f,
        27 => 0x30,
        28 => 0x28,
        29 => 0xe0,
        30 => 0x04,
        31 => 0x16,
        32 => 0x07,
        33 => 0x09,
        34 => 0x0a,
        35 => 0x0b,
        36 => 0x0d,
        37 => 0x0e,
        38 => 0x0f,
        39 => 0x33,
        40 => 0x34,
        41 => 0x35,
        42 => 0xe1,
        43 => 0x31,
        44 => 0x1d,
        45 => 0x1b,
        46 => 0x06,
        47 => 0x19,
        48 => 0x05,
        49 => 0x11,
        50 => 0x10,
        51 => 0x36,
        52 => 0x37,
        53 => 0x38,
        54 => 0xe5,
        55 => 0x55,
        56 => 0xe2,
        57 => 0x2c,
        58 => 0x39,
        59 => 0x3a,
        60 => 0x3b,
        61 => 0x3c,
        62 => 0x3d,
        63 => 0x3e,
        64 => 0x3f,
        65 => 0x40,
        66 => 0x41,
        67 => 0x42,
        68 => 0x43,
        69 => 0x53,
        70 => 0x47,
        71 => 0x5f,
        72 => 0x60,
        73 => 0x61,
        74 => 0x56,
        75 => 0x5c,
        76 => 0x5d,
        77 => 0x5e,
        78 => 0x57,
        79 => 0x59,
        80 => 0x5a,
        81 => 0x5b,
        82 => 0x62,
        83 => 0x63,
        86 => 0x64,
        87 => 0x44,
        88 => 0x45,
        96 => 0x58,
        97 => 0xe4,
        98 => 0x54,
        99 => 0x46,
        100 => 0xe6,
        102 => 0x4a,
        103 => 0x52,
        104 => 0x4b,
        105 => 0x50,
        106 => 0x4f,
        107 => 0x4d,
        108 => 0x51,
        109 => 0x4e,
        110 => 0x49,
        111 => 0x4c,
        119 => 0x48,
        125 => 0xe3,
        126 => 0xe7,
        127 => 0x65,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_keypad_and_right_modifier_without_collapsing() {
        assert_eq!(hid_usage(28), Some(0x28));
        assert_eq!(hid_usage(96), Some(0x58));
        assert_eq!(hid_usage(29), Some(0xe0));
        assert_eq!(hid_usage(97), Some(0xe4));
        assert_eq!(hid_usage(86), Some(0x64));
        assert_eq!(hid_usage(0xff), None);
    }
    #[test]
    fn drops_malformed_values_and_preserves_kernel_time() {
        let mut event = Event {
            kind: EV_KEY,
            code: 30,
            value: 1,
            time: libc::timeval {
                tv_sec: 9,
                tv_usec: 300,
            },
        };
        assert_eq!(decode(event).unwrap().at_us, 9_000_300);
        event.value = 3;
        assert!(decode(event).is_none());
        event.value = 1;
        event.time.tv_usec = 1_000_000;
        assert!(decode(event).is_none());
    }
    #[test]
    fn bitmap_uses_highest_word_first() {
        assert!(bitmap_has("0 40000000", 30));
        assert!(!bitmap_has("0 40000000", 28));
        assert!(!valid_event_name("eventx"));
    }
}
