//! Bounded read-only hidraw notification reception; poll does not query firmware.
use super::{
    HIDIOCGRAWINFO, RawInfo, Result, descriptor_from_fd, parse_descriptor, valid_hidraw_name,
};
use std::{
    ffi::{CStr, OsStr},
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::Path,
    time::Duration,
};

pub(crate) struct InputDevice {
    file: File,
}

impl InputDevice {
    pub(crate) fn open(path: &CStr) -> Result<Self> {
        let path = Path::new(OsStr::from_bytes(path.to_bytes()));
        if path.parent() != Some(Path::new("/dev"))
            || !path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(valid_hidraw_name)
        {
            return Err("Notification path must be a /dev/hidrawN node".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        let mut info = RawInfo::default();
        // SAFETY: writable UAPI structure, used only during the ioctl.
        if unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCGRAWINFO, &mut info) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let descriptor = descriptor_from_fd(file.as_raw_fd())?;
        let parsed =
            parse_descriptor(&descriptor).ok_or("Malformed notification HID descriptor")?;
        if info.vendor as u16 != 0x3151
            || !matches!(info.product as u16, 0x4011 | 0x4015)
            || !parsed.notification_input_valid
        {
            return Err(
                "Notification node is not the Nia87 report-ID-5 vendor input collection".into(),
            );
        }
        Ok(Self { file })
    }

    pub(crate) fn read_timeout(&mut self, timeout: Duration) -> Result<Option<[u8; 4]>> {
        let mut descriptor = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: single live descriptor; bounded wait supports caller cancellation.
        let status = unsafe { libc::poll(&mut descriptor, 1, timeout.as_millis().min(100) as i32) };
        if status < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(error.into());
        }
        if status == 0 {
            return Ok(None);
        }
        if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err("Keyboard notification collection disconnected".into());
        }
        // hidraw exposes the whole USB interface, including unrelated keyboard
        // and consumer reports. Only the vendor report ID is interpreted.
        let mut report = [0; 256];
        let count = match self.file.read(&mut report) {
            Ok(count) => count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        if count == 0 {
            return Err("Keyboard notification collection disconnected".into());
        }
        if report[0] != 5 {
            return Ok(None);
        }
        if count != 4 {
            return Err("Keyboard notification report was incomplete".into());
        }
        Ok(Some(
            report[..4].try_into().expect("four-byte report slice"),
        ))
    }
}
