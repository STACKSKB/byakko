//! Discovery selects a connection; each worker remains bound to its own target.
use super::discovery::{Availability, Discovery};
use byakko_core::{
    contract::{Command, Problem},
    editor::Status,
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
        if let Some(observed) = observed {
            let changed = observed != self.presence;
            self.presence = observed;
            let ready = match &self.presence {
                Availability::Ready { id } => Some(id.clone()),
                _ => None,
            };
            let read_failed = matches!(
                session.keymap().status(),
                Status::Unverified {
                    problem: Problem::Read(_)
                }
            );
            let needs_connection =
                self.worker.is_none() || ready.as_ref() != self.selected.as_ref() || read_failed;
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
        }
        if let Some(discovery) = &mut self.discovery {
            discovery.request();
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
