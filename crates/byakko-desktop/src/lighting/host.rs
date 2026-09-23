//! Screen host-lighting lifecycle; the sampler never owns HID.
use crate::{AutoRead, Closing, Desktop, Message as AppMessage};
use byakko_core::{
    lighting::HostSource,
    lighting::editor::Status,
    session::{
        Acceptance, Activity, ApplyFailure, HostPhase, HostTicket as CoreTicket, Recovery,
        Status as SessionStatus,
    },
};
use byakko_devices::{HostEvent, HostFrame, HostFrameError, HostMode, HostTicket};
use iced::Task;
use std::sync::mpsc::TryRecvError;

pub(crate) struct HostScreen {
    stream: crate::screen_stream::ScreenStream,
    mode_id: String,
}

impl Desktop {
    pub(crate) fn start_screen(&mut self, mode_id: String) {
        let offered = self.session.lighting().is_some_and(|editor| {
            editor
                .capabilities()
                .host_modes
                .iter()
                .any(|mode| mode.id == mode_id && mode.source == HostSource::ScreenAverage)
                && editor.status() == &Status::Ready
                && !editor.dirty()
        });
        if !offered || self.session.status() != &SessionStatus::Ready {
            self.notice = Some("Read verified lighting and save staged edits first".into());
            return;
        }
        match crate::screen_stream::ScreenStream::spawn() {
            Ok(stream) => self.screen = Some(HostScreen { stream, mode_id }),
            Err(error) => self.notice = Some(format!("Could not start screen capture: {error}")),
        }
    }

    pub(crate) fn stop_host(&mut self) {
        if let Ok(ticket) = self.session.request_host_stop() {
            if let Some(screen) = &self.screen {
                screen.stream.stop();
            }
            if let Some(executor) = &self.executor {
                executor.try_stop_host(device_ticket(ticket));
            }
        } else {
            self.screen = None;
        }
    }

    pub(crate) fn poll_screen(&mut self) {
        if matches!(
            self.session.activity(),
            Activity::HostLighting {
                phase: HostPhase::Stopping,
                ..
            }
        ) {
            return;
        }
        let Some(screen) = &self.screen else { return };
        let event = screen.stream.try_receive();
        match event {
            Ok(crate::screen_stream::Event::Ready) => {
                let mode_id = screen.mode_id.clone();
                let request = self.session.request_host_start(&mode_id);
                match request {
                    Ok(request) => {
                        let result = self
                            .executor
                            .as_ref()
                            .ok_or("Device executor is unavailable".into())
                            .and_then(|executor: &byakko_devices::Executor| {
                                executor
                                    .try_start_host(
                                        device_ticket(request.ticket),
                                        HostMode::Screen,
                                        request.expected,
                                    )
                                    .map_err(|error| {
                                        format!("Could not start host lighting: {error:?}")
                                    })
                            });
                        if let Err(message) = result {
                            self.session.accept_host_start_failed(
                                request.ticket,
                                ApplyFailure {
                                    message: message.clone(),
                                    recovery: Recovery::NotAttempted,
                                },
                            );
                            self.screen = None;
                            self.notice = Some(message);
                        }
                    }
                    Err(message) => {
                        self.screen = None;
                        self.notice = Some(message);
                    }
                }
            }
            Ok(crate::screen_stream::Event::Frame(rgb)) => {
                let Activity::HostLighting {
                    ticket,
                    phase: HostPhase::Streaming,
                    ..
                } = self.session.activity()
                else {
                    return;
                };
                if let Some(executor) = &self.executor {
                    match executor.try_send_host_frame(device_ticket(*ticket), HostFrame::Rgb(rgb))
                    {
                        Ok(()) | Err(HostFrameError::QueueFull) => {}
                        Err(error) => {
                            self.notice = Some(format!("Screen frame stopped: {error:?}"));
                            self.stop_host();
                        }
                    }
                }
            }
            Ok(crate::screen_stream::Event::Failed(reason)) => {
                self.notice = Some(format!("Screen capture stopped: {reason}"));
                self.stop_host();
            }
            Err(TryRecvError::Disconnected) => {
                self.notice = Some("Screen capture worker stopped".into());
                self.stop_host();
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    pub(crate) fn poll_host(&mut self) -> Option<Task<AppMessage>> {
        let event = self.executor.as_ref()?.try_receive_host();
        let event = match event {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                if let Activity::HostLighting { ticket, .. } = self.session.activity() {
                    let ticket = *ticket;
                    self.session.accept_host_finished(
                        ticket,
                        Err(ApplyFailure {
                            message: "Host device worker stopped before restoration was verified"
                                .into(),
                            recovery: Recovery::Unverified,
                        }),
                    );
                    self.screen = None;
                    self.closing = Closing::Open;
                    self.notice = Some("Host device worker stopped; lighting is unverified".into());
                    self.executor = None;
                    self.auto_read = AutoRead::ManualOnly;
                    return Some(Task::none());
                }
                return None;
            }
        };
        match event {
            HostEvent::Started { ticket } => {
                let accepted = self.session.accept_host_started(core_ticket(ticket));
                if accepted == Acceptance::Accepted
                    && matches!(
                        self.session.activity(),
                        Activity::HostLighting {
                            phase: HostPhase::Streaming,
                            ..
                        }
                    )
                {
                    if let Some(screen) = &self.screen {
                        screen.stream.start();
                    }
                } else if let Some(executor) = &self.executor {
                    executor.try_stop_host(ticket);
                }
            }
            HostEvent::StartFailed { ticket, failure } => {
                let message = failure.message.clone();
                if self
                    .session
                    .accept_host_start_failed(core_ticket(ticket), failure)
                    == Acceptance::Accepted
                {
                    self.notice = Some(message);
                    self.screen = None;
                    self.closing = Closing::Open;
                }
            }
            HostEvent::Finished { ticket, result } => {
                let restored = result.is_ok();
                let failure_message = result.as_ref().err().map(|failure| failure.message.clone());
                let accepted = self
                    .session
                    .accept_host_finished(core_ticket(ticket), result);
                if accepted == Acceptance::IgnoredStale {
                    return Some(Task::none());
                }
                if let Some(message) = failure_message {
                    self.notice = Some(message);
                }
                self.screen = None;
                if self.closing == Closing::Waiting {
                    if restored && self.session.status() == &SessionStatus::Ready {
                        return Some(self.close());
                    }
                    self.closing = Closing::Open;
                }
            }
        }
        Some(Task::none())
    }
}

fn device_ticket(ticket: CoreTicket) -> HostTicket {
    HostTicket {
        generation: ticket.generation,
        operation: ticket.operation,
    }
}

fn core_ticket(ticket: HostTicket) -> CoreTicket {
    CoreTicket {
        generation: ticket.generation,
        operation: ticket.operation,
    }
}
