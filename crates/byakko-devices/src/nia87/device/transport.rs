use super::Result;
use crate::hid::{
    HidApi, HidDevice,
    selection::{self, Identity},
};
use byakko_protocol::nia87::receiver::{self, RelayError, Status};
use serde::{Deserialize, Serialize};
use std::time::Duration;

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
    file.try_lock().map_err(lock_error)?;
    Ok(file)
}

fn lock_error(error: std::fs::TryLockError) -> Box<dyn std::error::Error + Send + Sync> {
    match error {
        std::fs::TryLockError::WouldBlock =>
            "The keyboard is busy with another Byakko operation. Wait for it to finish, then try again.".into(),
        std::fs::TryLockError::Error(error) => error.into(),
    }
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

impl Candidate {
    pub fn identity(&self) -> Identity {
        Identity {
            path: self.path.clone(),
            vendor_id: self.vid,
            product_id: self.pid,
            interface: self.interface,
            usage_page: self.usage_page,
            usage: self.usage,
        }
    }
}

/// Stable collection identity selected by discovery. Display strings are
/// deliberately excluded because they can be absent or vary by OS locale.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    inner: selection::Target,
}

impl Target {
    pub fn product_id(&self) -> u16 {
        self.inner.identity().product_id
    }

    pub fn from_candidate(
        candidate: &Candidate,
    ) -> std::result::Result<Self, TargetSelectionError> {
        if !is_nia87_configuration_candidate(&candidate.identity()) {
            return Err(TargetSelectionError::Unsupported);
        }
        Ok(Self {
            inner: selection::Target::from_identity(candidate.identity()),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetSelectionError {
    Missing,
    Ambiguous(usize),
    Changed,
    Unsupported,
    ReceiverSelectionRequired,
}

impl std::fmt::Display for TargetSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => f.write_str(
                "The keyboard was disconnected. Reconnect it and choose Read / reconnect.",
            ),
            Self::Ambiguous(count) => write!(
                f,
                "Found {count} compatible keyboards. Connect only the keyboard you want to configure."
            ),
            Self::Changed => {
                f.write_str("The keyboard connection changed. Choose Read / reconnect.")
            }
            Self::ReceiverSelectionRequired => f.write_str(
                "Receiver operations require a selected, board-validated Access connection.",
            ),
            Self::Unsupported => {
                f.write_str("This keyboard connection cannot be configured by Byakko.")
            }
        }
    }
}

impl std::error::Error for TargetSelectionError {}

impl Target {
    pub fn select(
        &self,
        candidates: &[Candidate],
    ) -> std::result::Result<Candidate, TargetSelectionError> {
        let identities = candidates
            .iter()
            .map(Candidate::identity)
            .collect::<Vec<_>>();
        self.inner
            .select(&identities)
            .map_err(TargetSelectionError::from)?;
        candidates
            .first()
            .cloned()
            .ok_or(TargetSelectionError::Missing)
    }
}

impl From<selection::SelectionError> for TargetSelectionError {
    fn from(error: selection::SelectionError) -> Self {
        match error {
            selection::SelectionError::Missing => Self::Missing,
            selection::SelectionError::Ambiguous(count) => Self::Ambiguous(count),
            selection::SelectionError::Changed => Self::Changed,
        }
    }
}

fn is_nia87_configuration_candidate(identity: &Identity) -> bool {
    identity.vendor_id == 0x3151
        && matches!(identity.product_id, 0x4011 | 0x4015)
        && identity.usage_page == 0xffff
        && identity.usage == 2
}

/// Result of a read-only OS HID enumeration for the Nia87 configuration
/// collection. Callers can decide how to present discovery without parsing
/// diagnostics or inferring state from a candidate count.
pub type Availability = selection::Availability<Candidate>;

/// Enumerate matching collections without opening a handle or sending reports.
pub fn availability() -> Availability {
    classify(candidates())
}

fn classify(result: Result<Vec<Candidate>>) -> Availability {
    selection::classify(result)
}

pub fn candidates() -> Result<Vec<Candidate>> {
    let api = HidApi::new()?;
    Ok(matching_candidates(&api))
}

fn matching_candidates(api: &HidApi) -> Vec<Candidate> {
    api.device_list()
        .filter(|d| is_nia87_configuration_candidate(&d.identity()))
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
        .collect()
}

pub fn open_unique() -> Result<(Candidate, HidDevice)> {
    let list = candidates()?;
    match list.len() {
        0 => return Err(TargetSelectionError::Missing.into()),
        1 => {}
        count => return Err(TargetSelectionError::Ambiguous(count).into()),
    }
    let candidate = list.into_iter().next().unwrap();
    let api = HidApi::new()?;
    let path = std::ffi::CString::new(candidate.path.as_str())?;
    let device = api.open_path(&path)?;
    Ok((candidate, device))
}

/// Re-enumerate once, require the selected collection identity to remain
/// unique, then open that exact path through the same OS enumeration handle.
pub fn open_expected(target: &Target) -> Result<(Candidate, HidDevice)> {
    let api = HidApi::new()?;
    let observed = matching_candidates(&api);
    let candidate = target.select(&observed)?;
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
        let request = byakko_protocol::nia87::protocol::read_request(opcode, 0, 0);
        let reply = read_payload(&device, opcode, 0, 0)?;
        if reply[0] != opcode {
            return Err(format!("Unexpected response to {opcode:02x}: {:02x?}", &reply[..]).into());
        }
        replies.push(
            serde_json::json!({"opcode":opcode,"payload":reply.as_slice(),"request":request.as_slice()}),
        );
    }
    Ok(serde_json::json!({"candidate":candidate,"replies":replies}))
}

// Production forwards directly. The explicitly enabled research build can
// simulate one transport failure while retaining a real device for recovery.
pub(super) trait FeatureSetter {
    fn send_setter(&self, report: &[u8]) -> Result<()>;
}

/// The logical keyboard setter was not submitted. Relay status/selection may
/// have been exchanged, but restoring a before-image would itself be a write.
#[derive(Debug)]
pub(super) struct SetterNotAttempted {
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl SetterNotAttempted {
    pub(super) fn new(source: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self { source }
    }
}

impl std::fmt::Display for SetterNotAttempted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.source, f)
    }
}

impl std::error::Error for SetterNotAttempted {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadPhase {
    SendRequest,
    ReceiveResponse,
}

#[derive(Debug)]
struct FeatureReadError {
    phase: ReadPhase,
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Display for FeatureReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phase = match self.phase {
            ReadPhase::SendRequest => "send request",
            ReadPhase::ReceiveResponse => "receive response",
        };
        write!(f, "HID feature read {phase} failed: {}", self.source)
    }
}

impl std::error::Error for FeatureReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

fn feature_read_exchange(
    request: &[u8; 65],
    reply: &mut [u8; 65],
    send: impl FnOnce(&[u8; 65]) -> crate::hid::Result<()>,
    receive: impl FnOnce(&mut [u8; 65]) -> crate::hid::Result<usize>,
) -> crate::hid::Result<usize> {
    send(request).map_err(|source| FeatureReadError {
        phase: ReadPhase::SendRequest,
        source,
    })?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    receive(reply).map_err(|source| {
        Box::new(FeatureReadError {
            phase: ReadPhase::ReceiveResponse,
            source,
        }) as Box<dyn std::error::Error + Send + Sync>
    })
}

// The receiver is a command relay. Its status and target selection are part
// of each exchange, while the ordinary Nia87 command and its pacing stay intact.
fn host_report(payload: [u8; 64]) -> [u8; 65] {
    let mut report = [0; 65];
    report[1..].copy_from_slice(&payload);
    report
}

fn receive_report(receive: &mut impl FnMut(&mut [u8; 65]) -> Result<usize>) -> Result<[u8; 65]> {
    let mut report = [0; 65];
    let length = receive(&mut report)?;
    if length != report.len() || report[0] != 0 {
        return Err(format!("Invalid receiver HID response length/prefix: {length}").into());
    }
    Ok(report)
}

fn receiver_status(
    send: &mut impl FnMut(&[u8; 65]) -> Result<()>,
    receive: &mut impl FnMut(&mut [u8; 65]) -> Result<usize>,
    wait: &mut impl FnMut(Duration),
) -> Result<Status> {
    send(&host_report(receiver::status_request()))?;
    wait(Duration::from_millis(receiver::STATUS_SETTLE_MS));
    let report = receive_report(receive)?;
    Ok(Status::decode(&report[1..])?)
}

fn receiver_exchange(
    command: &[u8; 65],
    expects_reply: bool,
    logical_send_started: &mut bool,
    mut send: impl FnMut(&[u8; 65]) -> Result<()>,
    mut receive: impl FnMut(&mut [u8; 65]) -> Result<usize>,
    mut wait: impl FnMut(Duration),
) -> Result<Option<[u8; 65]>> {
    for poll in 0..receiver::MAX_POLLS {
        let status = receiver_status(&mut send, &mut receive, &mut wait)?;
        status.before_command()?;
        if status.command_ready {
            break;
        }
        if poll + 1 == receiver::MAX_POLLS {
            return Err(RelayError::CommandTimeout.into());
        }
        wait(Duration::from_millis(receiver::POLL_INTERVAL_MS));
    }

    send(&host_report(receiver::select_keyboard_request()))?;
    *logical_send_started = true;
    send(command)?;
    if !expects_reply {
        return Ok(None);
    }

    for poll in 0..receiver::MAX_POLLS {
        // The radio needs this interval after the keyboard command and before
        // every reply-status query, including the first one.
        wait(Duration::from_millis(receiver::POLL_INTERVAL_MS));
        let status = receiver_status(&mut send, &mut receive, &mut wait)?;
        if !status.keyboard_online {
            return Err(RelayError::KeyboardOffline.into());
        }
        if status.reply_ready {
            send(&host_report(receiver::release_reply_request()))?;
            wait(Duration::from_millis(receiver::STATUS_SETTLE_MS));
            return Ok(Some(receive_report(&mut receive)?));
        }
        if poll + 1 == receiver::MAX_POLLS {
            return Err(RelayError::ReplyTimeout.into());
        }
    }
    unreachable!("receiver poll count is nonzero")
}

impl FeatureSetter for HidDevice {
    fn send_setter(&self, report: &[u8]) -> Result<()> {
        let mut logical_send_started = false;
        let submit = || {
            if self.product_id() == 0x4011 {
                let command: &[u8; 65] = report
                    .try_into()
                    .map_err(|_| "Nia87 feature report must be 65 bytes")?;
                if command[0] != 0 {
                    return Err("Nia87 feature report must have report ID zero".into());
                }
                receiver_exchange(
                    command,
                    false,
                    &mut logical_send_started,
                    |report| self.send_feature_report(report),
                    |reply| self.get_feature_report(reply),
                    std::thread::sleep,
                )?;
                Ok(())
            } else {
                logical_send_started = true;
                self.send_feature_report(report)
            }
        };
        #[cfg(feature = "research-tools")]
        let result = crate::research_fault::send(report, submit);
        #[cfg(not(feature = "research-tools"))]
        let result = {
            let mut submit = submit;
            submit()
        };
        // A failed logical send is uncertain; keep its recovery pacing. A
        // receiver rejection before that send needs no restore or setter wait.
        if result.is_err() && logical_send_started {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        result.map_err(|source| {
            if logical_send_started {
                source
            } else {
                Box::new(SetterNotAttempted::new(source))
            }
        })
    }
}

pub(super) fn read_payload(
    device: &HidDevice,
    opcode: u8,
    index: u8,
    page: u8,
) -> Result<[u8; 64]> {
    let mut request = [0u8; 65];
    request[1..].copy_from_slice(&byakko_protocol::nia87::protocol::read_request(
        opcode, index, page,
    ));
    let exchange = |reply: &mut [u8; 65]| {
        if device.product_id() == 0x4011 {
            *reply = receiver_exchange(
                &request,
                true,
                &mut false,
                |report| device.send_feature_report(report),
                |report| device.get_feature_report(report),
                std::thread::sleep,
            )?
            .expect("receiver read requests a reply");
            Ok(65)
        } else {
            feature_read_exchange(
                &request,
                reply,
                |request| device.send_feature_report(request),
                |reply| device.get_feature_report(reply),
            )
        }
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
    pub(super) fn open_for(selection: super::Selection<'_>) -> Result<Self> {
        Self::open_locked(transaction_lock()?, || {
            selection.open().map(|(_, device)| device)
        })
    }

    fn open_locked(lock: std::fs::File, open: impl FnOnce() -> Result<HidDevice>) -> Result<Self> {
        let device = open()?;
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
    use super::{
        Availability, Candidate, FeatureReadError, ReadPhase, Session, Target,
        TargetSelectionError, classify, feature_read_exchange, lock_error, lock_file,
    };

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
    fn target_selects_only_one_exact_collection_identity() {
        let expected = candidate("nia87-a");
        let target = Target::from_candidate(&expected).unwrap();
        let same_path = candidate("nia87-a");
        assert_eq!(
            target.select(std::slice::from_ref(&same_path)),
            Ok(same_path)
        );
        assert_eq!(target.select(&[]), Err(TargetSelectionError::Missing));

        let other_path = candidate("nia87-b");
        assert_eq!(
            target.select(std::slice::from_ref(&other_path)),
            Err(TargetSelectionError::Changed)
        );
        assert_eq!(
            target.select(&[expected.clone(), other_path.clone()]),
            Err(TargetSelectionError::Ambiguous(2))
        );

        let changed_metadata = expected;
        let changes: [fn(&mut Candidate); 5] = [
            |candidate: &mut Candidate| candidate.vid = 0,
            |candidate: &mut Candidate| candidate.pid = 0,
            |candidate: &mut Candidate| candidate.interface = 2,
            |candidate: &mut Candidate| candidate.usage_page = 0,
            |candidate: &mut Candidate| candidate.usage = 3,
        ];
        for change in changes {
            let mut changed = changed_metadata.clone();
            change(&mut changed);
            assert_eq!(
                target.select(std::slice::from_ref(&changed)),
                Err(TargetSelectionError::Changed)
            );
        }

        let mut unsupported = candidate("other");
        unsupported.usage = 1;
        assert_eq!(
            Target::from_candidate(&unsupported),
            Err(TargetSelectionError::Unsupported)
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

    #[test]
    fn lock_errors_preserve_operating_system_failures_instead_of_reporting_busy() {
        let error = lock_error(std::fs::TryLockError::Error(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "configuration lock permission denied",
        )));
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(error.to_string().contains("permission denied"));
        let busy = lock_error(std::fs::TryLockError::WouldBlock);
        assert!(busy.to_string().contains("keyboard is busy"));
    }

    #[test]
    fn failed_session_open_releases_lock_for_missing_target_and_io_failure() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "byakko-open-lock-test-{}-{stamp}",
            std::process::id()
        ));
        for error in [
            Box::new(TargetSelectionError::Missing) as Box<dyn std::error::Error + Send + Sync>,
            std::io::Error::new(std::io::ErrorKind::NotConnected, "keyboard disconnected").into(),
        ] {
            let reason = error.to_string();
            let result = Session::open_locked(lock_file(&path).unwrap(), || Err(error));
            assert_eq!(result.err().unwrap().to_string(), reason);
            assert!(
                lock_file(&path).is_ok(),
                "failed open must release the transaction"
            );
        }
    }

    fn receiver_status_report(reply_ready: bool, online: bool, command_ready: bool) -> [u8; 65] {
        let mut report = [0; 65];
        report[1] = u8::from(reply_ready);
        report[4] = u8::from(!online);
        report[6] = u8::from(command_ready);
        report[7] = 1;
        report
    }

    #[test]
    fn receiver_read_selects_keyboard_and_releases_only_its_new_reply() {
        use std::{cell::RefCell, collections::VecDeque, time::Duration};
        let request =
            super::host_report(byakko_protocol::nia87::protocol::read_request(0x80, 0, 0));
        let mut keyboard_reply = [0; 65];
        keyboard_reply[1..4].copy_from_slice(&[0x80, 0x00, 0x01]);
        let responses = RefCell::new(VecDeque::from([
            receiver_status_report(false, true, true),
            receiver_status_report(false, true, false),
            receiver_status_report(true, true, false),
            keyboard_reply,
        ]));
        let sent = RefCell::new(Vec::new());
        let waits = RefCell::new(Vec::new());
        let result = super::receiver_exchange(
            &request,
            true,
            &mut false,
            |report| {
                sent.borrow_mut().push(*report);
                Ok(())
            },
            |reply| {
                *reply = responses
                    .borrow_mut()
                    .pop_front()
                    .expect("scripted response");
                Ok(65)
            },
            |duration| waits.borrow_mut().push(duration),
        )
        .unwrap();
        assert_eq!(result, Some(keyboard_reply));
        assert!(responses.borrow().is_empty());
        assert_eq!(
            *sent.borrow(),
            vec![
                super::host_report(byakko_protocol::nia87::receiver::status_request()),
                super::host_report(byakko_protocol::nia87::receiver::select_keyboard_request()),
                request,
                super::host_report(byakko_protocol::nia87::receiver::status_request()),
                super::host_report(byakko_protocol::nia87::receiver::status_request()),
                super::host_report(byakko_protocol::nia87::receiver::release_reply_request()),
            ]
        );
        assert_eq!(
            *waits.borrow(),
            vec![
                Duration::from_millis(10),
                Duration::from_millis(100),
                Duration::from_millis(10),
                Duration::from_millis(100),
                Duration::from_millis(10),
                Duration::from_millis(10),
            ]
        );
    }

    #[test]
    fn receiver_retains_one_request_until_a_reply_after_the_initial_half_second() {
        use std::{cell::RefCell, collections::VecDeque};
        let request =
            super::host_report(byakko_protocol::nia87::protocol::read_request(0x87, 0, 0));
        let mut keyboard_reply = [0; 65];
        keyboard_reply[1] = 0x87;
        let mut responses = VecDeque::from([receiver_status_report(false, true, true)]);
        responses.extend(std::iter::repeat_n(
            receiver_status_report(false, true, false),
            5,
        ));
        responses.push_back(receiver_status_report(true, true, false));
        responses.push_back(keyboard_reply);
        let responses = RefCell::new(responses);
        let sent = RefCell::new(Vec::new());
        let waits = RefCell::new(Vec::new());
        let result = super::receiver_exchange(
            &request,
            true,
            &mut false,
            |report| {
                sent.borrow_mut().push(*report);
                Ok(())
            },
            |reply| {
                *reply = responses
                    .borrow_mut()
                    .pop_front()
                    .expect("scripted response");
                Ok(65)
            },
            |delay| waits.borrow_mut().push(delay),
        )
        .unwrap();
        assert_eq!(result, Some(keyboard_reply));
        assert!(responses.borrow().is_empty());
        let sent = sent.borrow();
        assert_eq!(sent.iter().filter(|report| report[1] == 0x87).count(), 1);
        assert_eq!(sent.iter().filter(|report| report[1] == 0xfc).count(), 1);
        assert_eq!(
            waits
                .borrow()
                .iter()
                .filter(|delay| **delay == std::time::Duration::from_millis(100))
                .count(),
            6
        );
    }
    #[test]
    fn receiver_rejects_stale_reply_or_sleep_before_keyboard_command() {
        use std::cell::RefCell;
        let request =
            super::host_report(byakko_protocol::nia87::protocol::read_request(0x80, 0, 0));
        for (status, expected) in [
            (
                receiver_status_report(true, true, true),
                byakko_protocol::nia87::receiver::RelayError::PendingReply,
            ),
            (
                receiver_status_report(false, false, true),
                byakko_protocol::nia87::receiver::RelayError::KeyboardOffline,
            ),
        ] {
            let sent = RefCell::new(Vec::new());
            let error = super::receiver_exchange(
                &request,
                true,
                &mut false,
                |report| {
                    sent.borrow_mut().push(*report);
                    Ok(())
                },
                |reply| {
                    *reply = status;
                    Ok(65)
                },
                |_| {},
            )
            .unwrap_err();
            assert_eq!(error.downcast_ref(), Some(&expected));
            assert_eq!(
                *sent.borrow(),
                vec![super::host_report(
                    byakko_protocol::nia87::receiver::status_request()
                )]
            );
        }
    }

    #[test]
    fn receiver_setter_waits_for_ready_then_returns_without_getting_keyboard_reply() {
        use std::cell::RefCell;
        let command = super::host_report([0x07; 64]);
        let sent = RefCell::new(Vec::new());
        let reads = RefCell::new(0);
        let result = super::receiver_exchange(
            &command,
            false,
            &mut false,
            |report| {
                sent.borrow_mut().push(*report);
                Ok(())
            },
            |reply| {
                *reads.borrow_mut() += 1;
                *reply = receiver_status_report(false, true, true);
                Ok(65)
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(result, None);
        assert_eq!(*reads.borrow(), 1);
        assert_eq!(
            *sent.borrow(),
            vec![
                super::host_report(byakko_protocol::nia87::receiver::status_request()),
                super::host_report(byakko_protocol::nia87::receiver::select_keyboard_request()),
                command,
            ]
        );
    }

    #[test]
    fn receiver_exchange_marks_only_the_logical_keyboard_send_as_attempted() {
        let command = super::host_report([0x07; 64]);
        let mut started = false;
        let error = super::receiver_exchange(
            &command,
            false,
            &mut started,
            |_| Ok(()),
            |reply| {
                *reply = receiver_status_report(false, false, true);
                Ok(65)
            },
            |_| {},
        )
        .unwrap_err();
        assert_eq!(
            error.downcast_ref(),
            Some(&byakko_protocol::nia87::receiver::RelayError::KeyboardOffline)
        );
        assert!(!started);

        let mut sent = 0;
        let error = super::receiver_exchange(
            &command,
            false,
            &mut started,
            |_| {
                sent += 1;
                if sent == 3 {
                    Err("logical HID send failed".into())
                } else {
                    Ok(())
                }
            },
            |reply| {
                *reply = receiver_status_report(false, true, true);
                Ok(65)
            },
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "logical HID send failed");
        assert_eq!(sent, 3);
        assert!(started);
    }

    #[test]
    fn receiver_poll_limits_stop_without_unrelated_reports() {
        use std::cell::RefCell;
        let request =
            super::host_report(byakko_protocol::nia87::protocol::read_request(0x80, 0, 0));
        for (ready, expected) in [
            (
                false,
                byakko_protocol::nia87::receiver::RelayError::CommandTimeout,
            ),
            (
                true,
                byakko_protocol::nia87::receiver::RelayError::ReplyTimeout,
            ),
        ] {
            let sent = RefCell::new(Vec::new());
            let reads = RefCell::new(0usize);
            let error = super::receiver_exchange(
                &request,
                true,
                &mut false,
                |report| {
                    sent.borrow_mut().push(*report);
                    Ok(())
                },
                |reply| {
                    let mut count = reads.borrow_mut();
                    *count += 1;
                    *reply = receiver_status_report(false, true, ready && *count == 1);
                    Ok(65)
                },
                |_| {},
            )
            .unwrap_err();
            assert_eq!(error.downcast_ref(), Some(&expected));
            let expected_reads = if ready {
                1 + byakko_protocol::nia87::receiver::MAX_POLLS
            } else {
                byakko_protocol::nia87::receiver::MAX_POLLS
            };
            assert_eq!(*reads.borrow(), expected_reads);
            let sent = sent.borrow();
            assert_eq!(sent.len(), expected_reads + if ready { 2 } else { 0 });
            assert_eq!(
                sent[0],
                super::host_report(byakko_protocol::nia87::receiver::status_request())
            );
            assert!(!sent.contains(&super::host_report(
                byakko_protocol::nia87::receiver::release_reply_request()
            )));
        }
    }

    #[test]
    fn feature_read_reports_request_phase_and_preserves_io_cause() {
        let mut reply = [0; 65];
        let error = feature_read_exchange(
            &[0; 65],
            &mut reply,
            |_| Err(std::io::Error::from_raw_os_error(71).into()),
            |_| panic!("receive must not run after a failed request"),
        )
        .unwrap_err();
        let context = error.downcast_ref::<FeatureReadError>().unwrap();
        assert_eq!(context.phase, ReadPhase::SendRequest);
        assert!(context.to_string().contains("send request"));
        assert_eq!(
            std::error::Error::source(context)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(71)
        );
    }

    #[test]
    fn feature_read_reports_response_phase_and_sends_once() {
        let sends = std::cell::Cell::new(0);
        let mut reply = [0; 65];
        let error = feature_read_exchange(
            &[0; 65],
            &mut reply,
            |_| {
                sends.set(sends.get() + 1);
                Ok(())
            },
            |_| Err(std::io::Error::from_raw_os_error(19).into()),
        )
        .unwrap_err();
        let context = error.downcast_ref::<FeatureReadError>().unwrap();
        assert_eq!(context.phase, ReadPhase::ReceiveResponse);
        assert_eq!(sends.get(), 1);
        assert_eq!(
            std::error::Error::source(context)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(19)
        );
    }
}
