//! Window input and clock adapter for the deterministic core recorder.
use super::{Desktop, Message as AppMessage, Page, macro_form::number};
use byakko_core::macros::recorder::{DelayPolicy, StopOutcome};
use iced::{
    Element, Event, Fill, Subscription, event,
    widget::{button, checkbox, column, container, row, text, text_input},
};
use std::time::Instant;

#[derive(Clone, Debug)]
pub(super) enum Message {
    Start,
    Stop,
    Fixed(bool),
    Delay(String),
    Input(Event, Instant),
}

pub(super) struct Options {
    fixed: bool,
    delay: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            fixed: false,
            delay: "50".into(),
        }
    }
}

impl Desktop {
    fn timestamp(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.clock)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    pub(super) fn finish_recording(&mut self, at: Instant) -> bool {
        match self.session.stop_macro_recording(self.timestamp(at)) {
            Ok(outcome) => {
                self.reset_macro_inputs();
                self.notice = Some(match outcome {
                    StopOutcome::Complete => "Recording staged; review events before saving.".into(),
                    StopOutcome::TimingClamped => "Recording stopped; the final held interval exceeded the supported range and was set to zero. Release events are staged.".into(),
                });
                true
            }
            Err(error) => {
                self.notice = Some(error);
                false
            }
        }
    }

    pub(super) fn update_recording(&mut self, message: Message) {
        match message {
            Message::Start if !self.busy() => {
                let policy = if self.recording_options.fixed {
                    number(&self.recording_options.delay, "Fixed delay").map(DelayPolicy::Fixed)
                } else {
                    self.session
                        .macros()
                        .map(|editor| DelayPolicy::Measured {
                            terminal_ms: 50.min(*editor.capabilities().delays_ms.end()),
                        })
                        .ok_or_else(|| "Macros are unavailable".to_string())
                };
                self.notice = policy
                    .and_then(|policy| self.session.start_macro_recording(policy))
                    .err();
                if self.notice.is_none() {
                    self.page = Page::Macros;
                }
            }
            Message::Stop if self.session.recording() => {
                self.finish_recording(Instant::now());
            }
            Message::Input(event, at) if self.session.recording() => {
                if matches!(event, Event::Window(iced::window::Event::Unfocused)) {
                    self.finish_recording(at);
                } else if let Some(action) = super::recording_input::action(&event)
                    && let Err(error) = self.session.record_macro_action(action, self.timestamp(at))
                    && self.finish_recording(at)
                {
                    self.notice = Some(format!(
                        "Recording stopped: {error}. Accepted events and held releases remain staged."
                    ));
                }
            }
            Message::Fixed(value) if !self.busy() => self.recording_options.fixed = value,
            Message::Delay(value) if !self.busy() => self.recording_options.delay = value,
            _ => {}
        }
    }
}

pub(super) fn subscription() -> Subscription<AppMessage> {
    event::listen_with(|event, status, _| {
        captures(&event, status).then(|| AppMessage::Record(Message::Input(event, Instant::now())))
    })
}

fn captures(event: &Event, status: event::Status) -> bool {
    matches!(event, Event::Window(iced::window::Event::Unfocused))
        || (status == event::Status::Ignored && super::recording_input::action(event).is_some())
}

pub(super) fn controls(app: &Desktop, editable: bool) -> Element<'_, AppMessage> {
    row![
        button("Record input")
            .on_press_maybe(editable.then_some(AppMessage::Record(Message::Start))),
        checkbox(app.recording_options.fixed)
            .label("Fixed wait")
            .on_toggle_maybe(editable.then_some(|value| AppMessage::Record(Message::Fixed(value)))),
        text_input("Wait (ms)", &app.recording_options.delay)
            .width(100)
            .on_input_maybe(
                (editable && app.recording_options.fixed)
                    .then_some(|value| AppMessage::Record(Message::Delay(value)))
            ),
        text("Appends to the draft"),
    ]
    .spacing(10)
    .into()
}

pub(super) fn capture_view(app: &Desktop) -> Element<'_, AppMessage> {
    let events = app
        .session
        .macros()
        .and_then(|editor| editor.draft())
        .map_or(0, |program| program.events.len());
    let mut content = column![
        text("Recording input").size(28),
        text("Type or click in this window. Keyboard and five mouse buttons are captured."),
        text("Stop or leave the window to finish. Held inputs receive release events."),
        text(format!("{events} events in draft · no device writes")),
        button("Stop recording").on_press(AppMessage::Record(Message::Stop)),
    ]
    .spacing(16);
    if let Some(notice) = &app.notice {
        content = content.push(text(notice));
    }
    container(content)
        .padding(30)
        .width(Fill)
        .height(Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widget_clicks_and_motion_are_excluded_but_focus_loss_always_finishes() {
        let click = Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left));
        assert!(captures(&click, event::Status::Ignored));
        assert!(!captures(&click, event::Status::Captured));
        assert!(!captures(
            &Event::Mouse(iced::mouse::Event::CursorMoved {
                position: iced::Point::ORIGIN
            }),
            event::Status::Ignored
        ));
        assert!(captures(
            &Event::Window(iced::window::Event::Unfocused),
            event::Status::Captured
        ));
    }
}
