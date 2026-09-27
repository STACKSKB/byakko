//! Prepare the OS source before startup; core owns the device lifecycle.
use super::sampler::{Event, Sampler};
use crate::form::host::Form;
use byakko_core::{
    model::lighting::{HostSource, Setting},
    session::Session,
    workflow::host::{HostOutcome, Phase},
};
use byakko_devices::Executor;
use byakko_devices::screen_sample::ScreenCapture;
use std::sync::mpsc::TryRecvError;

struct Intent {
    mode: String,
    setting: Option<Setting>,
}
enum State {
    Idle,
    Preparing { sampler: Sampler, intent: Intent },
    Ready { sampler: Sampler, intent: Intent },
    Attached(Sampler),
}
pub enum Outcome {
    None,
    Notice(String),
    Finished,
    Failed(String),
}
type Prepare = fn(HostSource, ScreenCapture) -> Result<Sampler, String>;
pub struct Controller {
    state: State,
    prepare: Prepare,
}
impl Default for Controller {
    fn default() -> Self {
        Self::new(Sampler::prepare)
    }
}
impl Controller {
    pub fn new(prepare: Prepare) -> Self {
        Self {
            state: State::Idle,
            prepare,
        }
    }
    pub fn busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }
    pub fn preparing(&self) -> bool {
        matches!(self.state, State::Preparing { .. } | State::Ready { .. })
    }
    pub fn start(
        &mut self,
        form: &Form,
        session: &mut Session,
        worker: &Executor,
    ) -> Result<(), String> {
        if self.busy() || session.busy() || session.recording() {
            return Err("Finish the current activity first".into());
        }
        let mode = session
            .lighting()
            .ok_or("Lighting is unavailable")?
            .capabilities()
            .host_modes
            .iter()
            .find(|mode| Some(&mode.id) == form.mode.as_ref())
            .ok_or("Select a host lighting mode")?;
        let sampler = (self.prepare)(mode.source, form.screen.clone())?;
        self.state = State::Preparing {
            sampler,
            intent: Intent {
                mode: mode.id.clone(),
                setting: form.setting.clone(),
            },
        };
        session.cancel_catalog();
        worker.cancel_catalog();
        Ok(())
    }
    pub fn stop(
        &mut self,
        session: &mut Session,
        worker: Option<&Executor>,
        mut problem: Option<String>,
    ) -> Outcome {
        if self.preparing() {
            self.state = State::Idle;
            return problem.map_or(
                Outcome::Notice("Host lighting cancelled.".into()),
                Outcome::Failed,
            );
        }
        if let State::Attached(sampler) = &self.state {
            sampler.stop();
            if let Ok(Event::Failed(reason)) = sampler.try_receive() {
                problem = problem.or(Some(reason));
            }
        }
        if let Some(ticket) = session.stop_host()
            && let Some(worker) = worker
        {
            worker.stop_host(ticket, problem);
        }
        Outcome::None
    }
    pub fn poll(&mut self, session: &mut Session, worker: Option<&Executor>) -> Outcome {
        let Some(worker) = worker else {
            if self.busy() {
                self.state = State::Idle;
                let _ = session.disconnect();
                return Outcome::Failed(
                    "Host lighting worker disconnected; restoration is unverified.".into(),
                );
            }
            return Outcome::None;
        };
        match worker.try_receive_host() {
            Ok(event) => match session.accept_host(event) {
                HostOutcome::Ignored | HostOutcome::Stopping => {}
                HostOutcome::Started => {
                    if let State::Attached(sampler) = &self.state
                        && let Err(reason) = sampler.begin()
                    {
                        return self.stop(session, Some(worker), Some(reason));
                    }
                    return Outcome::Notice("Host lighting is running.".into());
                }
                HostOutcome::Finished => {
                    self.state = State::Idle;
                    return Outcome::Finished;
                }
                HostOutcome::Failed(failure) => {
                    self.state = State::Idle;
                    return Outcome::Failed(crate::view::status::apply_failure(
                        "Could not run host lighting",
                        &failure,
                    ));
                }
            },
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) if self.busy() => {
                self.state = State::Idle;
                let _ = session.disconnect();
                return Outcome::Failed(
                    "Host lighting worker disconnected; restoration is unverified.".into(),
                );
            }
            Err(TryRecvError::Disconnected) => {}
        }
        let sampler = match &self.state {
            State::Idle => return Outcome::None,
            State::Preparing { sampler, .. }
            | State::Ready { sampler, .. }
            | State::Attached(sampler) => sampler,
        };
        match sampler.try_receive() {
            Ok(Event::Ready) if matches!(self.state, State::Preparing { .. }) => {
                if let State::Preparing { sampler, intent } =
                    std::mem::replace(&mut self.state, State::Idle)
                {
                    self.state = State::Ready { sampler, intent };
                }
            }
            Ok(Event::Frame(frame)) if session.host().phase() == Phase::Active => {
                if let Some(ticket) = session.host().ticket()
                    && let Err(reason) = worker.send_host_frame(ticket, frame)
                {
                    return self.stop(session, Some(worker), Some(reason));
                }
            }
            Ok(Event::Ready | Event::Frame(_)) | Err(TryRecvError::Empty) => {}
            Ok(Event::Failed(reason)) => return self.stop(session, Some(worker), Some(reason)),
            Err(TryRecvError::Disconnected) if session.host().phase() != Phase::Stopping => {
                return self.stop(
                    session,
                    Some(worker),
                    Some("Host sampler disconnected".into()),
                );
            }
            Err(TryRecvError::Disconnected) => {}
        }
        if matches!(self.state, State::Ready { .. }) && !session.catalog_scanning() {
            let State::Ready { sampler, intent } = std::mem::replace(&mut self.state, State::Idle)
            else {
                unreachable!()
            };
            let request = match session.start_host(&intent.mode, intent.setting) {
                Ok(request) => request,
                Err(reason) => return Outcome::Failed(reason),
            };
            self.state = State::Attached(sampler);
            if let Err(event) = worker.start_host(request) {
                let outcome = session.accept_host(event);
                self.state = State::Idle;
                return match outcome {
                    HostOutcome::Failed(failure) => Outcome::Failed(failure.message),
                    _ => Outcome::Failed("Host startup was rejected".into()),
                };
            }
            return Outcome::Notice("Starting host lighting…".into());
        }
        Outcome::None
    }
}
