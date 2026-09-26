//! Advertised host modes, local source choices, and explicit start/stop.
use crate::{
    form::host::{Displays, Form, Message},
    widget::{
        lighting,
        panels::{self, UiStyle},
    },
};
use byakko_core::{
    editor::{Editor, Status, lighting::LightingRules},
    model::lighting::HostSource,
    projection::lighting::parameter_controls,
    workflow::host::Phase,
};
use byakko_devices::screen_sample::ScreenSampling;
use iced::{
    Element,
    widget::{button, column, pick_list, row, slider, text},
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct DisplayChoice {
    id: Option<String>,
    label: String,
}
impl std::fmt::Display for DisplayChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
pub fn view<'a>(
    form: &'a Form,
    editor: &Editor<LightingRules>,
    idle: bool,
    preparing: bool,
    phase: Phase,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    if editor.capabilities().host_modes.is_empty() {
        return column![].into();
    }
    let busy = preparing || phase != Phase::Idle;
    let editable = idle && !busy;
    let modes = row(editor.capabilities().host_modes.iter().map(|mode| {
        panels::selectable_button(
            style,
            mode.label.clone(),
            form.mode.as_deref() == Some(&mode.id),
            editable.then(|| Message::Mode(mode.id.clone())),
        )
    }))
    .spacing(style.spacing.s)
    .wrap();
    let mut content = column![text("Host lighting"), modes].spacing(style.spacing.s);
    if let Some(mode) = editor
        .capabilities()
        .host_modes
        .iter()
        .find(|mode| Some(&mode.id) == form.mode.as_ref())
    {
        if let (Some(parameters), Some(setting)) = (&mode.parameters, &form.setting) {
            match parameter_controls(&parameters.schema, setting) {
                Ok(controls) => {
                    content = content.push(lighting::parameters(
                        setting,
                        controls,
                        editable,
                        style,
                        format!("host:{}", mode.id),
                        Message::Parameter,
                        |_| Message::Picker,
                    ))
                }
                Err(reason) => content = content.push(text(reason)),
            }
        }
        if mode.source == HostSource::ScreenAverage {
            content = content.push(screen(form, editable, style));
        }
    }
    let ready = editable
        && editor.status() == &Status::Ready
        && !editor.dirty()
        && editor.draft().is_some()
        && form.mode.is_some();
    content
        .push(
            row![
                button("Start").on_press_maybe(ready.then_some(Message::Start)),
                button("Stop & restore").on_press_maybe(busy.then_some(Message::Stop)),
                text(if preparing {
                    "Preparing source…"
                } else {
                    match phase {
                        Phase::Idle => "",
                        Phase::Starting => "Starting…",
                        Phase::Active => "Running",
                        Phase::Stopping => "Restoring onboard lighting…",
                    }
                }),
            ]
            .spacing(style.spacing.s),
        )
        .into()
}
fn screen<'a>(form: &'a Form, editable: bool, style: &'a UiStyle) -> Element<'a, Message> {
    let mut choices = vec![DisplayChoice {
        id: None,
        label: "Primary display".into(),
    }];
    if let Displays::Ready(displays) = &form.displays {
        choices.extend(displays.iter().map(|display| DisplayChoice {
            id: Some(display.id.clone()),
            label: display.label.clone(),
        }));
    }
    let selected = choices
        .iter()
        .find(|choice| choice.id == form.screen.display_id)
        .cloned();
    let mut controls = column![].spacing(style.spacing.s);
    if editable {
        controls = controls.push(
            row![
                pick_list(choices, selected, |choice: DisplayChoice| Message::Display(
                    choice.id
                ))
                .placeholder("Select display"),
                button("Refresh displays").on_press_maybe(
                    (!matches!(form.displays, Displays::Loading))
                        .then_some(Message::RefreshDisplays)
                ),
                button("Average screen").on_press(Message::Sampling(ScreenSampling::Average)),
                button("Sample point")
                    .on_press(Message::Sampling(ScreenSampling::Point { x: 500, y: 500 })),
            ]
            .spacing(style.spacing.s),
        );
    } else {
        controls = controls.push(text(
            selected.map_or_else(|| "Selected display".into(), |choice| choice.label),
        ));
    }
    if let Displays::Failed(reason) = &form.displays {
        controls = controls.push(text(reason));
    }
    if let ScreenSampling::Point { x, y } = form.screen.sampling {
        controls = controls.push(text(format!("Screen point: {x}, {y} (0–1000)")));
        if editable {
            controls = controls
                .push(slider(0..=1000, x, move |x| {
                    Message::Sampling(ScreenSampling::Point { x, y })
                }))
                .push(slider(0..=1000, y, move |y| {
                    Message::Sampling(ScreenSampling::Point { x, y })
                }));
        }
    }
    controls.into()
}
