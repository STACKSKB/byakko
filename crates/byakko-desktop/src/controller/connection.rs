//! Discovery selects a connection; each worker remains bound to its own target.
use super::discovery::{Availability, Discovery};
use byakko_core::{
    contract::Command,
    session::{Outcome, Session},
};
use byakko_devices::{Executor, Retirement};

pub type Attach = dyn Fn(Option<&str>) -> Result<(String, Executor), String>;

enum Refresh {
    Idle,
    Reading,
}
enum Reconnect {
    Manual,
    Discovered(String),
}
pub enum RefreshStep {
    Inactive,
    Finished,
    Command(Result<Box<Command>, String>),
}

pub struct Connection {
    worker: Option<Executor>,
    retiring: Option<Retirement>,
    reconnect: Option<Reconnect>,
    selected: Option<String>,
    presence: Availability,
    discovery: Option<Discovery>,
    attach: Box<Attach>,
    refresh: Refresh,
}

impl Connection {
    pub fn new(attach: Box<Attach>) -> Self {
        Self {
            worker: None,
            retiring: None,
            reconnect: None,
            selected: None,
            presence: Availability::Unknown,
            discovery: None,
            attach,
            refresh: Refresh::Idle,
        }
    }
    pub fn monitor(&mut self, discovery: Discovery) {
        self.discovery = Some(discovery);
    }
    pub fn presence(&self) -> &Availability {
        &self.presence
    }
    pub fn executor(&self) -> Option<&Executor> {
        self.worker.as_ref()
    }
    pub fn settling(&self) -> bool {
        self.retiring.is_some() || self.reconnect.is_some()
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
    pub fn retire(&mut self, session: &mut Session) -> Result<(), String> {
        self.invalidate_discovery();
        self.refresh = Refresh::Idle;
        self.reconnect = None;
        self.selected = None;
        if let Some(worker) = self.worker.take() {
            self.retiring = Some(worker.retire());
        }
        session.disconnect().map(|_| ())
    }
    /// An explicit read reselects the current collection even if no scan saw removal.
    pub fn read(&mut self, session: &mut Session) -> Result<Option<Command>, String> {
        self.retire(session)?;
        self.reconnect = Some(Reconnect::Manual);
        self.poll(session)
    }
    /// Finish the old worker before opening another device transaction.
    pub fn poll(&mut self, session: &mut Session) -> Result<Option<Command>, String> {
        if self
            .retiring
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return Ok(None);
        }
        self.retiring = None;
        match self.reconnect.take() {
            Some(Reconnect::Manual) => self.attach(session, None).map(Some),
            Some(Reconnect::Discovered(id)) => self.attach(session, Some(&id)).map(Some),
            None => Ok(None),
        }
    }
    fn attach(&mut self, session: &mut Session, expected: Option<&str>) -> Result<Command, String> {
        let (id, worker) = (self.attach)(expected)?;
        if expected.is_some_and(|expected| expected != id) {
            return Err("The discovered keyboard changed before attachment".into());
        }
        worker.set_generation(session.connect()?);
        self.selected = Some(id.clone());
        self.presence = Availability::Ready { id };
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
        if let Some(observed) = observed
            && let Some(command) = self.observe(session, observed)?
        {
            return Ok(Some(command));
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
        let changed = observed != self.presence;
        self.presence = observed;
        let ready = match &self.presence {
            Availability::Ready { id } => Some(id.clone()),
            _ => None,
        };
        // An unchanged selected collection remains connected after a read
        // failure. Reattempting its receiver identity requires explicit Read.
        let needs_connection = self.worker.is_none() || ready.as_ref() != self.selected.as_ref();
        if ready.is_none() && changed {
            self.retire(session)?;
        } else if let Some(id) = ready
            && needs_connection
        {
            if self.worker.is_some() {
                self.retire(session)?;
            }
            if !session.requires_manual_read() {
                self.reconnect = Some(Reconnect::Discovered(id));
                return self.poll(session);
            }
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
            assert!(expected.is_none_or(|id| id == "receiver"));
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

    #[test]
    fn unchanged_candidate_does_not_retry_failed_first_read_but_manual_read_does() {
        let (mut link, mut session, attachments) = fixture();
        let ready = Availability::Ready {
            id: "receiver".into(),
        };
        let command = link.observe(&mut session, ready.clone()).unwrap().unwrap();
        complete_failed_first_read(&mut link, &mut session, command);
        for _ in 0..3 {
            assert!(link.observe(&mut session, ready.clone()).unwrap().is_none());
        }
        assert_eq!(attachments.load(Ordering::SeqCst), 1);
        let mut command = link.read(&mut session).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while command.is_none() {
            assert!(Instant::now() < deadline, "manual reconnect did not settle");
            std::thread::yield_now();
            command = link.poll(&mut session).unwrap();
        }
        assert_eq!(attachments.load(Ordering::SeqCst), 2);
        assert!(link.executor().is_some());
        complete_failed_first_read(&mut link, &mut session, command.unwrap());
    }

    #[test]
    fn missing_then_ready_candidate_reconnects_after_failed_first_read() {
        let (mut link, mut session, attachments) = fixture();
        let ready = Availability::Ready {
            id: "receiver".into(),
        };
        let command = link.observe(&mut session, ready.clone()).unwrap().unwrap();
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
        let command = link.observe(&mut session, ready).unwrap().unwrap();
        assert_eq!(attachments.load(Ordering::SeqCst), 2);
        complete_failed_first_read(&mut link, &mut session, command);
    }
}
