//! Coordinates local recording with the window clock and an outstanding scan.
use crate::{
    form::recording::{Message, Options},
    input::recording,
};
use byakko_core::{
    recorder::macros::{DelayPolicy, StopOutcome},
    session::Session,
};
use byakko_devices::Executor;
use iced::Event;
use std::time::Instant;

pub struct Controller {
    options: Options,
    requested: Option<DelayPolicy>,
    clock: Instant,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            options: Options::default(),
            requested: None,
            clock: Instant::now(),
        }
    }
}

impl Controller {
    pub fn options(&self) -> &Options {
        &self.options
    }
    pub fn pending(&self) -> bool {
        self.requested.is_some()
    }

    pub fn update(
        &mut self,
        message: Message,
        session: &mut Session,
        worker: Option<&Executor>,
    ) -> Result<Option<String>, String> {
        match message {
            Message::Stop => self.finish(session, Instant::now()).map(Some),
            _ if session.recording() || self.pending() => Ok(None),
            Message::Start => {
                if session.busy() {
                    return Err("Wait for the device operation before recording".into());
                }
                let editor = session.macros().ok_or("Macros are unavailable")?;
                let policy = self.options.policy(editor.capabilities())?;
                if session.catalog_scanning() {
                    let worker = worker.ok_or("The device worker is unavailable")?;
                    self.requested = Some(policy);
                    worker.cancel_catalog();
                    Ok(Some("Finishing the library read before recording…".into()))
                } else {
                    session.start_recording(policy)?;
                    Ok(Some("Recording input in this window.".into()))
                }
            }
            Message::Fixed(fixed) => {
                self.options.fixed = fixed;
                Ok(None)
            }
            Message::Delay(delay) => {
                self.options.delay = delay;
                Ok(None)
            }
        }
    }

    pub fn catalog_finished(&mut self, session: &mut Session) -> Result<Option<String>, String> {
        let Some(policy) = self.requested.take() else {
            return Ok(None);
        };
        session.start_recording(policy)?;
        Ok(Some("Recording input in this window.".into()))
    }

    pub fn finish(&mut self, session: &mut Session, at: Instant) -> Result<String, String> {
        self.requested = None;
        if !session.recording() {
            return Ok("Recording cancelled.".into());
        }
        match session.stop_recording(self.timestamp(at))? {
            StopOutcome::Complete => Ok("Recording staged; review the events before saving.".into()),
            StopOutcome::TimingClamped => Ok("Recording stopped; the final held interval exceeded the supported range and was set to zero. Release events are staged.".into()),
        }
    }

    pub fn input(
        &mut self,
        event: &Event,
        at: Instant,
        session: &mut Session,
    ) -> Result<Option<String>, String> {
        if self.pending() && matches!(event, Event::Window(iced::window::Event::Unfocused)) {
            return self.finish(session, at).map(Some);
        }
        if !session.recording() {
            return Ok(None);
        }
        if matches!(event, Event::Window(iced::window::Event::Unfocused)) {
            return self.finish(session, at).map(Some);
        }
        if let Some(action) = recording::action(event)
            && let Err(reason) = session.record_input(action, self.timestamp(at))
        {
            return match self.finish(session, at) {
                Ok(_) => Ok(Some(format!(
                    "Recording stopped: {reason}. Accepted events and held releases remain staged."
                ))),
                Err(stop) => Err(format!(
                    "Input rejected: {reason}. Could not finish recording: {stop}"
                )),
            };
        }
        Ok(None)
    }

    fn timestamp(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.clock)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
}
