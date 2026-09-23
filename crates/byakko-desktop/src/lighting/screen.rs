//! Local capture controls. Display identities belong to the OS sampler, not HID.
use crate::{Message as AppMessage, control_widgets, panels::UiStyle};
use byakko_devices::screen_sample::{DisplaySource, ScreenCapture, ScreenSampling};
use iced::{
    Element, Task,
    widget::{button, column, text},
};

#[derive(Clone, Debug)]
pub(crate) enum Message {
    Refresh,
    Discovered(Result<Vec<DisplaySource>, String>),
    Display(Option<String>),
    Sampling(ScreenSampling),
}

#[derive(Default)]
enum Inventory {
    #[default]
    Unloaded,
    Loading,
    Ready(Vec<DisplaySource>),
    Failed(String),
}

#[derive(Default)]
pub(crate) struct Controls {
    pub capture: ScreenCapture,
    inventory: Inventory,
}

fn message(value: Message) -> AppMessage {
    AppMessage::Lighting(super::Message::Screen(value))
}

impl Controls {
    pub fn update(&mut self, value: Message, editable: bool) -> Task<AppMessage> {
        match value {
            Message::Discovered(result) if matches!(self.inventory, Inventory::Loading) => {
                self.inventory = match result {
                    Ok(displays) => Inventory::Ready(displays),
                    Err(reason) => Inventory::Failed(reason),
                };
            }
            Message::Discovered(_) => {}
            _ if !editable => {}
            Message::Refresh if !matches!(self.inventory, Inventory::Loading) => {
                self.inventory = Inventory::Loading;
                let (sender, receiver) = iced::futures::channel::oneshot::channel();
                if let Err(error) = std::thread::Builder::new()
                    .name("byakko-displays".into())
                    .spawn(move || {
                        let _ = sender.send(byakko_devices::screen_sample::displays());
                    })
                {
                    self.inventory = Inventory::Failed(error.to_string());
                    return Task::none();
                }
                return Task::perform(
                    async move {
                        receiver
                            .await
                            .unwrap_or_else(|_| Err("Display discovery stopped".into()))
                    },
                    |result| message(Message::Discovered(result)),
                );
            }
            Message::Refresh => {}
            Message::Display(id) => {
                if id.is_none()
                    || matches!(&self.inventory, Inventory::Ready(displays) if displays.iter().any(|display| Some(&display.id) == id.as_ref()))
                {
                    self.capture.display_id = id;
                }
            }
            Message::Sampling(sampling) => {
                if !matches!(sampling, ScreenSampling::Point { x, y } if x > 1000 || y > 1000) {
                    self.capture.sampling = sampling;
                }
            }
        }
        Task::none()
    }

    pub fn view(&self, style: &UiStyle, editable: bool) -> Element<'static, AppMessage> {
        let mut choices = vec![control_widgets::Choice {
            label: "Primary / default display".into(),
            selected: self.capture.display_id.is_none(),
            message: editable.then(|| message(Message::Display(None))),
        }];
        if let Inventory::Ready(displays) = &self.inventory {
            choices.extend(displays.iter().map(|display| control_widgets::Choice {
                label: display.label.clone(),
                selected: self.capture.display_id.as_ref() == Some(&display.id),
                message: editable.then(|| message(Message::Display(Some(display.id.clone())))),
            }));
        }
        let mut controls = column![
            control_widgets::choices(style, "Capture display", choices),
            button("Find / refresh displays").on_press_maybe(
                (editable && !matches!(self.inventory, Inventory::Loading))
                    .then(|| message(Message::Refresh))
            ),
        ]
        .spacing(style.spacing.s);
        match &self.inventory {
            Inventory::Loading => controls = controls.push(text("Finding displays…")),
            Inventory::Failed(reason) => controls = controls.push(text(reason.clone())),
            _ => {}
        }
        if let Some(id) = &self.capture.display_id {
            controls = controls.push(text(format!("Selected display: {id}")));
        }
        controls = controls.push(control_widgets::choices(
            style,
            "Sample",
            [
                control_widgets::Choice {
                    label: "Display average".into(),
                    selected: matches!(self.capture.sampling, ScreenSampling::Average),
                    message: editable.then(|| message(Message::Sampling(ScreenSampling::Average))),
                },
                control_widgets::Choice {
                    label: "Point on display".into(),
                    selected: matches!(self.capture.sampling, ScreenSampling::Point { .. }),
                    message: editable.then(|| {
                        message(Message::Sampling(ScreenSampling::Point { x: 500, y: 500 }))
                    }),
                },
            ],
        ));
        if let ScreenSampling::Point { x, y } = self.capture.sampling {
            controls = controls
                .push(text("Position: 0 is left/top, 1000 is right/bottom."))
                .push(control_widgets::level(
                    style,
                    "Horizontal",
                    0..=1000,
                    x,
                    editable.then_some(move |x| {
                        message(Message::Sampling(ScreenSampling::Point { x, y }))
                    }),
                ))
                .push(control_widgets::level(
                    style,
                    "Vertical",
                    0..=1000,
                    y,
                    editable.then_some(move |y| {
                        message(Message::Sampling(ScreenSampling::Point { x, y }))
                    }),
                ));
        }
        controls.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_retains_missing_selection_and_edits_freeze_during_streaming() {
        let mut controls = Controls {
            capture: ScreenCapture::default(),
            inventory: Inventory::Loading,
        };
        let source = DisplaySource {
            id: "second".into(),
            label: "Second display".into(),
        };
        let _ = controls.update(Message::Discovered(Ok(vec![source])), false);
        let _ = controls.update(Message::Display(Some("second".into())), true);
        let _ = controls.update(
            Message::Sampling(ScreenSampling::Point { x: 1000, y: 0 }),
            true,
        );
        let _ = controls.update(Message::Display(None), false);
        let _ = controls.update(Message::Sampling(ScreenSampling::Average), false);
        assert_eq!(controls.capture.display_id.as_deref(), Some("second"));
        assert!(matches!(
            controls.capture.sampling,
            ScreenSampling::Point { x: 1000, y: 0 }
        ));
        controls.inventory = Inventory::Loading;
        let _ = controls.update(Message::Discovered(Ok(vec![])), true);
        assert_eq!(controls.capture.display_id.as_deref(), Some("second"));
        let _ = controls.update(Message::Display(Some("unknown".into())), true);
        let _ = controls.update(
            Message::Sampling(ScreenSampling::Point { x: 1001, y: 0 }),
            true,
        );
        assert_eq!(controls.capture.display_id.as_deref(), Some("second"));
        assert!(matches!(
            controls.capture.sampling,
            ScreenSampling::Point { x: 1000, y: 0 }
        ));
    }
}
