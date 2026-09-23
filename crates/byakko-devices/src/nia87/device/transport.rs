use super::Result;
use crate::hid::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};

// OS-held lock survives neither crashes nor process exit. The empty lock file
// stays on disk; existence alone never means that a transaction is active.
pub(super) fn transaction_lock() -> Result<std::fs::File> {
    let path = std::env::temp_dir().join("byakko-nia87-configuration.lock");
    lock_file(&path)
}

fn lock_file(path: &std::path::Path) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .map_err(|_| "Another Byakko transaction is active; retry after it finishes")?;
    Ok(file)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// Result of a read-only OS HID enumeration for the Nia87 configuration
/// collection. Callers can decide how to present discovery without parsing
/// diagnostics or inferring state from a candidate count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability {
    Unavailable,
    Available(Candidate),
    Ambiguous(Vec<Candidate>),
    EnumerationFailed(String),
}

/// Enumerate matching collections without opening a handle or sending reports.
pub fn availability() -> Availability {
    classify(candidates())
}

fn classify(result: Result<Vec<Candidate>>) -> Availability {
    match result {
        Ok(candidates) => match candidates.as_slice() {
            [] => Availability::Unavailable,
            [candidate] => Availability::Available(candidate.clone()),
            _ => Availability::Ambiguous(candidates),
        },
        Err(error) => Availability::EnumerationFailed(error.to_string()),
    }
}

pub fn candidates() -> Result<Vec<Candidate>> {
    let api = HidApi::new()?;
    Ok(api
        .device_list()
        .filter(|d| {
            d.vendor_id() == 0x3151
                && matches!(d.product_id(), 0x4011 | 0x4015)
                && d.usage_page() == 0xffff
                && d.usage() == 2
        })
        .map(|d| Candidate {
            path: d.path().to_string_lossy().into_owned(),
            vid: d.vendor_id(),
            pid: d.product_id(),
            interface: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            manufacturer: d.manufacturer_string().map(str::to_owned),
            product: d.product_string().map(str::to_owned),
        })
        .collect())
}

pub fn open_unique() -> Result<(Candidate, HidDevice)> {
    let list = candidates()?;
    if list.len() != 1 {
        return Err(format!(
            "Expected one Nia87 configuration collection; found {}. Connect one keyboard by USB.",
            list.len()
        )
        .into());
    }
    let candidate = list.into_iter().next().unwrap();
    let api = HidApi::new()?;
    let path = std::ffi::CString::new(candidate.path.as_str())?;
    let device = api.open_path(&path)?;
    Ok((candidate, device))
}

pub fn descriptor() -> Result<serde_json::Value> {
    let (candidate, device) = open_unique()?;
    let mut bytes = [0u8; 4096];
    let len = device.get_report_descriptor(&mut bytes)?;
    Ok(
        serde_json::json!({"candidate":candidate,"report_descriptor_hex":bytes[..len].iter().map(|b|format!("{b:02x}")).collect::<Vec<_>>().join(" "),"length":len}),
    )
}

/// Two read-only requests verified against the Nia87-specific vendor call chain.
/// Sending a feature report selects the read operation; it does not mutate settings.
pub fn inspect() -> Result<serde_json::Value> {
    let _lock = transaction_lock()?;
    let (candidate, device) = open_unique()?;
    let mut replies = Vec::new();
    for opcode in [0x80u8, 0x85] {
        let mut request = [0u8; 65];
        request[1] = opcode;
        // BIT7 framing hypothesis: complement of bytes 0..6 at payload offset 7.
        // Restricted to known read-only opcodes until verified against the device.
        request[8] = !opcode;
        device.send_feature_report(&request)?;
        std::thread::sleep(std::time::Duration::from_millis(30));
        let mut reply = [0u8; 65];
        let n = device.get_feature_report(&mut reply)?;
        if n != 65 || reply[0] != 0 || reply[1] != opcode {
            return Err(format!(
                "Unexpected response to {opcode:02x}: length={n} bytes={:02x?}",
                &reply[..n]
            )
            .into());
        }
        replies.push(
            serde_json::json!({"opcode":opcode,"payload":&reply[1..],"request":&request[1..]}),
        );
    }
    Ok(serde_json::json!({"candidate":candidate,"replies":replies}))
}

// Production forwards directly. The explicitly enabled research build can
// simulate one transport failure while retaining a real device for recovery.
pub(super) trait FeatureSetter {
    fn send_setter(&self, report: &[u8]) -> Result<()>;
}

impl FeatureSetter for HidDevice {
    fn send_setter(&self, report: &[u8]) -> Result<()> {
        #[cfg(feature = "research-tools")]
        let result = crate::research_fault::send(report, || self.send_feature_report(report));
        #[cfg(not(feature = "research-tools"))]
        let result = self.send_feature_report(report);
        // An error does not prove that the firmware did not receive the setter.
        // Callers normally sleep only after success; retain the longest known
        // setter interval before any recovery request on an uncertain result.
        if result.is_err() {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        result
    }
}

pub(super) fn read_payload(
    device: &HidDevice,
    opcode: u8,
    index: u8,
    page: u8,
) -> Result<[u8; 64]> {
    let mut request = [0u8; 65];
    request[1..].copy_from_slice(&crate::nia87::protocol::read_request(opcode, index, page));
    let exchange = |reply: &mut [u8; 65]| {
        device.send_feature_report(&request)?;
        std::thread::sleep(std::time::Duration::from_millis(30));
        device.get_feature_report(reply)
    };
    #[cfg(feature = "research-tools")]
    let (n, reply) = crate::research_trace::read(&request, exchange)?;
    #[cfg(not(feature = "research-tools"))]
    let (n, reply) = {
        let mut reply = [0u8; 65];
        (exchange(&mut reply)?, reply)
    };
    if n != 65 || reply[0] != 0 {
        return Err(format!("Invalid HID response length/prefix: {n}").into());
    }
    Ok(reply[1..].try_into()?)
}

/// One HID handle held under the same OS lock for a complete operation.
/// Fields drop in declaration order: close the device before releasing the lock.
pub(super) struct Session {
    device: HidDevice,
    _lock: std::fs::File,
}

impl Session {
    pub(super) fn open() -> Result<Self> {
        let lock = transaction_lock()?;
        let (_, device) = open_unique()?;
        Ok(Self {
            device,
            _lock: lock,
        })
    }

    pub(super) fn device(&self) -> &HidDevice {
        &self.device
    }
}

#[cfg(test)]
mod tests {
    use super::{Availability, Candidate, classify, lock_file};

    fn candidate(path: &str) -> Candidate {
        Candidate {
            path: path.into(),
            vid: 0x3151,
            pid: 0x4011,
            interface: 1,
            usage_page: 0xffff,
            usage: 2,
            manufacturer: None,
            product: Some("Nia87".into()),
        }
    }

    #[test]
    fn availability_classifies_zero_one_and_multiple_collections() {
        assert_eq!(classify(Ok(Vec::new())), Availability::Unavailable);

        let only = candidate("nia87-a");
        assert_eq!(
            classify(Ok(vec![only.clone()])),
            Availability::Available(only)
        );

        let first = candidate("nia87-a");
        let second = candidate("nia87-b");
        assert_eq!(
            classify(Ok(vec![first.clone(), second.clone()])),
            Availability::Ambiguous(vec![first, second])
        );
    }

    #[test]
    fn enumeration_failure_is_a_distinct_availability_state() {
        let failure = classify(Err("permission denied".into()));
        assert_eq!(
            failure,
            Availability::EnumerationFailed("permission denied".into())
        );
        assert_ne!(failure, Availability::Unavailable);
        assert_ne!(failure, Availability::Ambiguous(Vec::new()));
    }

    #[test]
    fn operating_system_lock_excludes_second_handle_and_releases_on_drop() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("byakko-lock-test-{}-{stamp}", std::process::id()));
        let first = lock_file(&path).unwrap();
        assert!(lock_file(&path).is_err());
        drop(first);
        assert!(lock_file(&path).is_ok());
        // Retain the empty test artifact in accordance with the no-deletion rule.
    }
}
