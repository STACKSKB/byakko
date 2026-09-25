//! Local capture controls. Display identities belong to the OS sampler, not HID.
use crate::{
    Message as AppMessage,
    panels::{self, UiStyle},
};
use byakko_devices::screen_sample::{DisplaySource, ScreenCapture, ScreenSampling};
use iced::{
    Alignment, Element, Task,
    widget::{button, column, pick_list, row, slider, text},
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayChoice {
    id: Option<String>,
    label: String,
}

impl std::fmt::Display for DisplayChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
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
        let mut displays = vec![DisplayChoice {
            id: None,
            label: "Primary display".into(),
        }];
        if let Inventory::Ready(sources) = &self.inventory {
            displays.extend(sources.iter().map(|display| DisplayChoice {
                id: Some(display.id.clone()),
                label: display.label.clone(),
            }));
        }
        let selected = displays
            .iter()
            .find(|display| display.id == self.capture.display_id)
            .cloned()
            .unwrap_or_else(|| DisplayChoice {
                id: self.capture.display_id.clone(),
                label: "Selected display (unavailable)".into(),
            });
        if !displays.contains(&selected) {
            displays.push(selected.clone());
        }
        if !editable {
            displays.retain(|display| display == &selected);
        }
        let mut controls = column![
            row![
                text("Display").width(style.fields.compact),
                crate::clipped_dropdown::clipped(
                    pick_list(displays, Some(selected), |display: DisplayChoice| message(
                        Message::Display(display.id)
                    ))
                    .width(style.fields.regular)
                    .into()
                ),
                button("Refresh").on_press_maybe(
                    (editable && !matches!(self.inventory, Inventory::Loading))
                        .then(|| message(Message::Refresh))
                ),
            ]
            .spacing(style.spacing.s)
            .align_y(Alignment::Center),
            row![
                text("Sample").width(style.fields.compact),
                panels::selectable_button(
                    style,
                    "Average",
                    matches!(self.capture.sampling, ScreenSampling::Average),
                    editable.then(|| message(Message::Sampling(ScreenSampling::Average)))
                ),
                panels::selectable_button(
                    style,
                    "Point",
                    matches!(self.capture.sampling, ScreenSampling::Point { .. }),
                    editable.then(|| message(Message::Sampling(ScreenSampling::Point {
                        x: 500,
                        y: 500
                    })))
                ),
            ]
            .spacing(style.spacing.s)
            .align_y(Alignment::Center),
        ]
        .spacing(style.spacing.s);
        match &self.inventory {
            Inventory::Loading => controls = controls.push(text("Finding displays...")),
            Inventory::Failed(reason) => controls = controls.push(text(reason.clone())),
            _ => {}
        }
        if let ScreenSampling::Point { x, y } = self.capture.sampling {
            for (label, value, horizontal) in [("Horizontal", x, true), ("Vertical", y, false)] {
                let control: Element<'static, AppMessage> = if editable {
                    slider(0..=1000, value, move |value| {
                        message(Message::Sampling(if horizontal {
                            ScreenSampling::Point { x: value, y }
                        } else {
                            ScreenSampling::Point { x, y: value }
                        }))
                    })
                    .step(1u16)
                    .width(style.fields.regular)
                    .into()
                } else {
                    iced::widget::space().width(style.fields.regular).into()
                };
                controls = controls.push(
                    row![
                        text(label).width(style.fields.compact),
                        control,
                        text(format!("{}%", value / 10)),
                    ]
                    .spacing(style.spacing.s)
                    .align_y(Alignment::Center),
                );
            }
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
