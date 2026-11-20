//! Discovery selects a connection; each worker remains bound to its own target.
use super::discovery::{Availability, Discovery, Endpoint};
use byakko_core::{
    contract::Command,
    session::{Outcome, Session},
};
use byakko_devices::{Executor, Retirement};
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};

type Attached = Result<(String, Executor), String>;
pub type Attach = dyn Fn(Option<&str>) -> Attached + Send + Sync;

enum Attachment {
    Idle,
    Queued,
    Opening(JoinHandle<Attached>),
}

enum Refresh {
    Idle,
    Reading,
}
pub enum RefreshStep {
    Inactive,
    Finished,
    Command(Result<Box<Command>, String>),
}

pub struct Connection {
    worker: Option<Executor>,
    retiring: Option<Retirement>,
    attachment: Attachment,
    // The pin outlives the worker. Only a deliberate choice can replace it.
    selected: Option<String>,
    presence: Availability,
    discovery: Option<Discovery>,
    scan_requested: bool,
    attach: Arc<Attach>,
    refresh: Refresh,
}

impl Connection {
    pub fn new(attach: Box<Attach>) -> Self {
        Self {
            worker: None,
            retiring: None,
            attachment: Attachment::Idle,
            selected: None,
            presence: Availability::Unknown,
            discovery: None,
            scan_requested: false,
            attach: Arc::from(attach),
            refresh: Refresh::Idle,
        }
    }
    pub fn monitor(&mut self, discovery: Discovery) {
        self.discovery = Some(discovery);
    }
    pub fn presence(&self) -> &Availability {
        &self.presence
    }
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }
    pub fn executor(&self) -> Option<&Executor> {
        self.worker.as_ref()
    }
    pub fn settling(&self) -> bool {
        self.retiring.is_some() || self.attaching()
    }
    pub fn attaching(&self) -> bool {
        !matches!(self.attachment, Attachment::Idle)
    }
    pub fn monitoring(&self) -> bool {
        self.discovery.is_some()
    }
    pub fn awaiting_discovery(&self) -> bool {
        self.presence == Availability::Unknown
    }
    pub fn invalidate_discovery(&mut self) {
        if let Some(discovery) = &mut self.discovery {
            discovery.invalidate();
        }
    }
    pub fn request_scan(&mut self) {
        if let Some(discovery) = &mut self.discovery {
            // A result already waiting in the channel may predate a new plug-in.
            discovery.invalidate();
            self.scan_requested = discovery.request();
        }
    }
    pub fn scan_requested(&self) -> bool {
        self.scan_requested
    }
    pub fn retire(&mut self, session: &mut Session) -> Result<(), String> {
        if matches!(self.attachment, Attachment::Opening(_)) {
            return Err("Wait for the keyboard connection to finish opening.".into());
        }
        self.invalidate_discovery();
        self.refresh = Refresh::Idle;
        self.attachment = Attachment::Idle;
        if let Some(worker) = self.worker.take() {
            self.retiring = Some(worker.retire());
        }
        session.disconnect().map(|_| ())
    }
    /// Manual refresh retries the pinned endpoint even if discovery saw a replacement.
    pub fn read(&mut self, session: &mut Session) -> Result<Option<Command>, String> {
        if self.attaching() {
            return Ok(None);
        }
        if self.selected.is_none() {
            match &self.presence {
                Availability::Choices { .. } => {
                    return Err("Choose Wired USB or 2.4 GHz receiver to read the keyboard.".into());
                }
                Availability::Ready(endpoint) => self.selected = Some(endpoint.id.clone()),
                _ => {}
            }
        }
        self.retire(session)?;
        self.attachment = Attachment::Queued;
        self.poll(session)
    }
    /// Start a read-only connection to the exact endpoint named by the control.
    pub fn choose(&mut self, session: &mut Session, id: &str) -> Result<Option<Command>, String> {
        if !self.presence.contains(id) {
            return Err("This keyboard connection is no longer available.".into());
        }
        if self.settling() {
            return Err("Wait for the previous keyboard operation to finish.".into());
        }
        let switching = self.selected().is_some_and(|pinned| pinned != id);
        self.retire(session)?;
        if switching {
            session.reset_target()?;
        }
        self.selected = Some(id.to_owned());
        self.attachment = Attachment::Queued;
        self.poll(session)
    }
    /// Wait for the old device worker to retire, then open its replacement off the UI thread.
    pub fn poll(&mut self, session: &mut Session) -> Result<Option<Command>, String> {
        if self
            .retiring
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return Ok(None);
        }
        self.retiring = None;
        self.attachment = match std::mem::replace(&mut self.attachment, Attachment::Idle) {
            Attachment::Idle => Attachment::Idle,
            Attachment::Queued => {
                let attach = Arc::clone(&self.attach);
                let expected = self.selected.clone();
                let worker = thread::Builder::new()
                    .name("keyboard-attachment".into())
                    .spawn(move || attach(expected.as_deref()))
                    .map_err(|error| format!("Could not start keyboard connection: {error}"))?;
                Attachment::Opening(worker)
            }
            Attachment::Opening(worker) if !worker.is_finished() => Attachment::Opening(worker),
            Attachment::Opening(worker) => {
                let (id, worker) = worker
                    .join()
                    .map_err(|_| "Keyboard attachment stopped unexpectedly".to_owned())??;
                return self.accept_attachment(session, id, worker).map(Some);
            }
        };
        Ok(None)
    }
    fn accept_attachment(
        &mut self,
        session: &mut Session,
        id: String,
        worker: Executor,
    ) -> Result<Command, String> {
        if self.selected().is_some_and(|expected| expected != id) {
            self.retiring = Some(worker.retire());
            return Err("The discovered keyboard changed before attachment".into());
        }
        worker.set_generation(session.connect()?);
        if !self.presence.contains(&id) {
            // Manual read before the first scan, or a pin that returned between scans.
            self.presence = Availability::Ready(Endpoint {
                id: id.clone(),
                label: "Keyboard",
            });
            self.request_scan();
        }
        self.selected = Some(id);
        self.worker = Some(worker);
        self.refresh = Refresh::Reading;
        session.read()
    }
    /// Called only while device, file and recording activities are idle.
    pub fn scan(&mut self, session: &mut Session) -> Result<Option<Command>, String> {
        if self.settling() {
            return self.poll(session);
        }
        let observed = self.discovery.as_mut().and_then(Discovery::receive);
        if let Some(observed) = observed {
            let requested = std::mem::take(&mut self.scan_requested);
            let command = self.observe(session, observed)?;
            if command.is_some() || requested || self.settling() {
                return Ok(command);
            }
        }
        if let Some(discovery) = &mut self.discovery {
            discovery.request();
        }
        Ok(None)
    }
    fn observe(
        &mut self,
        session: &mut Session,
        observed: Availability,
    ) -> Result<Option<Command>, String> {
        self.presence = observed;
        let available = match (&self.selected, &self.presence) {
            (Some(id), presence) => presence.contains(id),
            (None, Availability::Ready(endpoint)) => {
                self.selected = Some(endpoint.id.clone());
                true
            }
            _ => false,
        };
        if self.worker.is_some() {
            if !available {
                self.retire(session)?;
            }
        } else if available && !session.requires_manual_read() {
            self.attachment = Attachment::Queued;
            return self.poll(session);
        }
        Ok(None)
    }
    pub fn lost(&mut self, session: &mut Session) -> Result<(), String> {
        self.presence = Availability::Unknown;
        self.retire(session)
    }
    /// Successful reads advance the one-pass refresh. Any failure ends it.
    pub fn advance(&mut self, session: &mut Session, outcome: &Outcome) -> RefreshStep {
        if matches!(self.refresh, Refresh::Idle) || matches!(outcome, Outcome::Ignored) {
            return RefreshStep::Inactive;
        }
        if !matches!(
            outcome,
            Outcome::Loaded
                | Outcome::LightingLoaded
                | Outcome::SettingsLoaded
                | Outcome::PictureLoaded
                | Outcome::MacroLoaded
        ) {
            self.refresh = Refresh::Idle;
            return RefreshStep::Finished;
        }
        match session.refresh_next() {
            Ok(Some(command)) => RefreshStep::Command(Ok(Box::new(command))),
            Ok(None) => {
                self.refresh = Refresh::Idle;
                if session.macros().is_some() {
                    RefreshStep::Command(session.request_macro_catalog().map(Box::new))
                } else {
                    RefreshStep::Finished
                }
            }
            Err(reason) => {
                self.refresh = Refresh::Idle;
                RefreshStep::Command(Err(reason))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{
        contract::ApplyFailure,
        editor::Status,
        model::keymap::{Change, State},
    };
    use byakko_devices::{Device, memory};
    use std::{
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    struct FailedIdentity;
    impl Device for FailedIdentity {
        fn read(&mut self) -> Result<State, String> {
            Err("Receiver board identity unavailable".into())
        }
        fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
            unreachable!("read-only connection fixture")
        }
    }

    fn fixture() -> (Connection, Session, Arc<AtomicUsize>) {
        let attachments = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&attachments);
        let link = Connection::new(Box::new(move |expected| {
            assert_eq!(
                expected,
                Some("receiver"),
                "only the selected receiver may open"
            );
            calls.fetch_add(1, Ordering::SeqCst);
            let worker = Executor::spawn(FailedIdentity, PathBuf::new())
                .map_err(|error| error.to_string())?;
            Ok(("receiver".into(), worker))
        }));
        let session = memory::demo().unwrap().session().unwrap();
        (link, session, attachments)
    }

    fn complete_failed_first_read(link: &mut Connection, session: &mut Session, command: Command) {
        let worker = link.executor().unwrap();
        worker.try_submit(command).unwrap();
        let completion = worker.receive(Some(Duration::from_secs(2))).unwrap();
        let outcome = session.accept(completion);
        assert!(matches!(outcome, Outcome::Failed(_)));
        assert!(matches!(
            link.advance(session, &outcome),
            RefreshStep::Finished
        ));
        assert!(matches!(
            session.keymap().status(),
            Status::Unverified {
                problem: byakko_core::contract::Problem::Read(_)
            }
        ));
    }

    fn wait_for_read(link: &mut Connection, session: &mut Session) -> Command {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(command) = link.poll(session).unwrap() {
                return command;
            }
            assert!(Instant::now() < deadline, "attachment did not settle");
            thread::yield_now();
        }
    }

    #[test]
    fn offline_receiver_stays_pinned_with_wired_present_and_manual_read_retries_it() {
        let (mut link, mut session, attachments) = fixture();
        let both = Availability::Choices {
            wired: Endpoint {
                id: "wired".into(),
                label: "Wired USB",
            },
            receiver: Endpoint {
                id: "receiver".into(),
                label: "2.4 GHz receiver",
            },
        };
        assert!(link.observe(&mut session, both.clone()).unwrap().is_none());
        assert!(link.choose(&mut session, "receiver").unwrap().is_none());
        let command = wait_for_read(&mut link, &mut session);
        complete_failed_first_read(&mut link, &mut session, command);
        for _ in 0..3 {
            assert!(link.observe(&mut session, both.clone()).unwrap().is_none());
        }
        assert_eq!(attachments.load(Ordering::SeqCst), 1);
        assert!(link.read(&mut session).unwrap().is_none());
        let command = wait_for_read(&mut link, &mut session);
        assert_eq!(attachments.load(Ordering::SeqCst), 2);
        assert!(link.executor().is_some());
        complete_failed_first_read(&mut link, &mut session, command);
    }

    #[test]
    fn missing_then_ready_candidate_reconnects_after_failed_first_read() {
        let (mut link, mut session, attachments) = fixture();
        let ready = Availability::Ready(Endpoint {
            id: "receiver".into(),
            label: "2.4 GHz receiver",
        });
        assert!(link.observe(&mut session, ready.clone()).unwrap().is_none());
        let command = wait_for_read(&mut link, &mut session);
        complete_failed_first_read(&mut link, &mut session, command);
        assert!(
            link.observe(&mut session, Availability::Missing)
                .unwrap()
                .is_none()
        );
        assert!(link.executor().is_none());
        let deadline = Instant::now() + Duration::from_secs(2);
        while link.settling() {
            assert!(
                Instant::now() < deadline,
                "disconnected worker did not retire"
            );
            std::thread::yield_now();
            assert!(link.poll(&mut session).unwrap().is_none());
        }
        assert!(link.observe(&mut session, ready).unwrap().is_none());
        let command = wait_for_read(&mut link, &mut session);
        assert_eq!(attachments.load(Ordering::SeqCst), 2);
        complete_failed_first_read(&mut link, &mut session, command);
    }

    #[test]
    fn an_attachment_returning_another_id_never_allocates_a_generation() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = attempts.clone();
        let mut link = Connection::new(Box::new(move |expected| {
            assert_eq!(expected, Some("receiver"));
            let id = if count.fetch_add(1, Ordering::SeqCst) == 0 {
                "wired"
            } else {
                "receiver"
            };
            let worker = Executor::spawn(memory::demo()?, PathBuf::new())
                .map_err(|error| error.to_string())?;
            Ok((id.into(), worker))
        }));
        let mut session = memory::demo().unwrap().session().unwrap();
        let ready = Availability::Ready(Endpoint {
            id: "receiver".into(),
            label: "2.4 GHz receiver",
        });
        assert!(link.observe(&mut session, ready).unwrap().is_none());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match link.poll(&mut session) {
                Err(reason) => {
                    assert_eq!(reason, "The discovered keyboard changed before attachment");
                    break;
                }
                Ok(result) => assert!(result.is_none(), "the wrong device may never be read"),
            }
            assert!(Instant::now() < deadline, "attachment did not settle");
            thread::yield_now();
        }
        assert_eq!(link.selected(), Some("receiver"));
        assert!(link.executor().is_none());
        assert!(!session.busy());
        assert!(link.read(&mut session).unwrap().is_none());
        let read = wait_for_read(&mut link, &mut session);
        assert_eq!(read.generation, 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}
