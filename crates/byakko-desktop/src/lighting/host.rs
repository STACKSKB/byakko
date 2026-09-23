//! Host-lighting lifecycle; OS samplers never own HID.
use crate::{AutoRead, Closing, Desktop, Message as AppMessage};
use byakko_core::{
    lighting::editor::Status,
    lighting::{HostMode as LightingHostMode, HostSource},
    session::{
        Acceptance, Activity, ApplyFailure, HostPhase, HostTicket as CoreTicket, Recovery,
        Status as SessionStatus,
    },
};
use byakko_devices::{HostEvent, HostFrame, HostFrameError, HostTicket};
use iced::Task;
use std::sync::mpsc::TryRecvError;

enum Sampler {
    Screen(crate::screen_stream::ScreenStream),
    Audio(crate::audio_stream::AudioStream),
}

enum SampleEvent {
    Ready,
    Frame(HostFrame),
    Failed(String),
}

pub(crate) struct HostInput {
    stream: Sampler,
    mode: LightingHostMode,
}

impl HostInput {
    fn spawn(mode: LightingHostMode) -> Result<Self, String> {
        let stream = match mode.source {
            HostSource::ScreenAverage => Sampler::Screen(
                crate::screen_stream::ScreenStream::spawn().map_err(|error| error.to_string())?,
            ),
            HostSource::PlaybackAudio { bands: 32 } => Sampler::Audio(
                crate::audio_stream::AudioStream::spawn().map_err(|error| error.to_string())?,
            ),
            HostSource::PlaybackAudio { .. } => {
                return Err("This audio sampler currently supports 32 bands".into());
            }
        };
        Ok(Self { stream, mode })
    }

    fn stop(&self) {
        match &self.stream {
            Sampler::Screen(stream) => stream.stop(),
            Sampler::Audio(stream) => stream.stop(),
        }
    }

    fn start(&self) -> Result<(), String> {
        match &self.stream {
            Sampler::Screen(stream) => {
                stream.start();
                Ok(())
            }
            Sampler::Audio(stream) => stream.start(),
        }
    }

    fn try_receive(&self) -> Result<SampleEvent, TryRecvError> {
        match &self.stream {
            Sampler::Screen(stream) => stream.try_receive().map(|event| match event {
                crate::screen_stream::Event::Ready => SampleEvent::Ready,
                crate::screen_stream::Event::Frame(rgb) => SampleEvent::Frame(HostFrame::Rgb(rgb)),
                crate::screen_stream::Event::Failed(reason) => SampleEvent::Failed(reason),
            }),
            Sampler::Audio(stream) => stream.try_receive().map(|event| match event {
                crate::audio_stream::Event::Ready { .. } => SampleEvent::Ready,
                crate::audio_stream::Event::Frame(bands) => {
                    SampleEvent::Frame(HostFrame::Bands(bands.to_vec()))
                }
                crate::audio_stream::Event::Failed(reason) => SampleEvent::Failed(reason),
            }),
        }
    }
}

impl Desktop {
    pub(crate) fn start_host(&mut self, mode_id: String) {
        let offered = self
            .session
            .lighting()
            .filter(|editor| editor.status() == &Status::Ready && !editor.dirty())
            .and_then(|editor| {
                editor
                    .capabilities()
                    .host_modes
                    .iter()
                    .find(|mode| mode.id == mode_id)
            })
            .cloned();
        let Some(mode) = offered.filter(|_| self.session.status() == &SessionStatus::Ready) else {
            self.notice = Some("Read verified lighting and save staged edits first".into());
            return;
        };
        match HostInput::spawn(mode) {
            Ok(host) => self.host = Some(host),
            Err(error) => self.notice = Some(format!("Could not start host capture: {error}")),
        }
    }

    pub(crate) fn stop_host(&mut self) {
        if let Ok(ticket) = self.session.request_host_stop() {
            if let Some(host) = &self.host {
                host.stop();
            }
            if let Some(executor) = &self.executor {
                executor.try_stop_host(device_ticket(ticket));
            } else {
                self.session.accept_host_finished(
                    ticket,
                    Err(ApplyFailure {
                        message:
                            "Host device worker is unavailable; lighting restoration is unverified"
                                .into(),
                        recovery: Recovery::Unverified,
                    }),
                );
                self.host = None;
                self.closing = Closing::Open;
            }
        } else {
            self.host = None;
        }
    }

    pub(crate) fn poll_host_input(&mut self) {
        if matches!(
            self.session.activity(),
            Activity::HostLighting {
                phase: HostPhase::Stopping,
                ..
            }
        ) {
            return;
        }
        let Some(host) = &self.host else { return };
        let event = host.try_receive();
        match event {
            Ok(SampleEvent::Ready) => {
                let mode_id = host.mode.id.clone();
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
                                        request.mode,
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
                            self.host = None;
                            self.notice = Some(message);
                        }
                    }
                    Err(message) => {
                        self.host = None;
                        self.notice = Some(message);
                    }
                }
            }
            Ok(SampleEvent::Frame(frame)) => {
                let Activity::HostLighting {
                    ticket,
                    phase: HostPhase::Streaming,
                    ..
                } = self.session.activity()
                else {
                    return;
                };
                if let Some(executor) = &self.executor {
                    match executor.try_send_host_frame(device_ticket(*ticket), frame) {
                        Ok(()) | Err(HostFrameError::QueueFull) => {}
                        Err(error) => {
                            self.notice = Some(format!("Host frame stopped: {error:?}"));
                            self.stop_host();
                        }
                    }
                }
            }
            Ok(SampleEvent::Failed(reason)) => {
                self.notice = Some(format!("Host capture stopped: {reason}"));
                self.stop_host();
            }
            Err(TryRecvError::Disconnected) => {
                self.notice = Some("Host capture worker stopped".into());
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
                    self.host = None;
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
                    if let Some(host) = &self.host {
                        if let Err(reason) = host.start() {
                            self.notice = Some(format!("Host capture stopped: {reason}"));
                            self.stop_host();
                        }
                    } else {
                        self.stop_host();
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
                    self.host = None;
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
                self.host = None;
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
