//! Fail-closed udev helper for the observed Nia87 configuration collection.
//! This reads only the sysfs HID report descriptor; it never opens a device node.
use std::{env, fs::File, io::Read, path::PathBuf, process::ExitCode};

const WINDOWS_OBSERVED: [u8; 20] = [
    0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01, 0x09, 0x02, 0x15, 0x80, 0x25, 0x7f, 0x75, 0x08, 0x95,
    0x40, 0xb1, 0x02, 0xc0,
];
const LINUX_OBSERVED: [u8; 20] = [
    0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01, 0x09, 0x02, 0x15, 0x80, 0x25, 0x7f, 0x95, 0x40, 0x75,
    0x08, 0xb1, 0x02, 0xc0,
];
const MAX_DESCRIPTOR: u64 = 4096;
const MAX_HIDRAW_DIGITS: usize = 5;

fn valid_name(name: &str) -> bool {
    let Some(digits) = name.strip_prefix("hidraw") else {
        return false;
    };
    !digits.is_empty()
        && digits.len() <= MAX_HIDRAW_DIGITS
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn matches_descriptor(bytes: &[u8]) -> bool {
    bytes == WINDOWS_OBSERVED || bytes == LINUX_OBSERVED
}

fn check(name: &str) -> bool {
    if !valid_name(name) {
        return false;
    }
    let path = PathBuf::from("/sys/class/hidraw")
        .join(name)
        .join("device/report_descriptor");
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_DESCRIPTOR + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return false;
    }
    bytes.len() <= MAX_DESCRIPTOR as usize && matches_descriptor(&bytes)
}

fn main() -> ExitCode {
    let mut args = env::args_os();
    let _program = args.next();
    let Some(name) = args.next() else {
        return ExitCode::FAILURE;
    };
    if args.next().is_some() {
        return ExitCode::FAILURE;
    }
    let Some(name) = name.to_str() else {
        return ExitCode::FAILURE;
    };
    if !check(name) {
        return ExitCode::FAILURE;
    }
    println!("nia87-config");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_observed_descriptor_only() {
        assert!(matches_descriptor(&WINDOWS_OBSERVED));
        assert!(matches_descriptor(&LINUX_OBSERVED));
        let keyboard = [0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0xc0];
        assert!(!matches_descriptor(&keyboard));
        let mut extra = WINDOWS_OBSERVED.to_vec();
        extra.push(0);
        assert!(!matches_descriptor(&extra));
        assert!(!matches_descriptor(&WINDOWS_OBSERVED[..19]));
        let mut wrong_count = LINUX_OBSERVED;
        wrong_count[14] = 0x3f;
        assert!(!matches_descriptor(&wrong_count));
        let mut wrong_size = LINUX_OBSERVED;
        wrong_size[16] = 0x07;
        assert!(!matches_descriptor(&wrong_size));
        let mut reordered = LINUX_OBSERVED;
        reordered.swap(7, 9);
        assert!(!matches_descriptor(&reordered));
    }
    #[test]
    fn strict_hidraw_name_rejects_traversal_and_extra_text() {
        assert!(valid_name("hidraw0"));
        assert!(valid_name("hidraw12345"));
        for name in [
            "hidraw",
            "hidraw123456",
            "hidraw-1",
            "hidraw1/../x",
            "../hidraw0",
            "/dev/hidraw0",
            "hidraw1x",
            "HIDRAW1",
            "hidraw１",
        ] {
            assert!(!valid_name(name), "{name}");
        }
    }
}
