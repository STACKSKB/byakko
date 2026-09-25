//! Render capability-projected lighting controls; device policy lives in core.
mod host;
pub(crate) mod live;
pub(crate) mod screen;
use super::{Desktop, Message as AppMessage};
use crate::panels::{self, UiStyle};
use byakko_core::lighting::{
    Color, Content, Edit,
    controls::{self, Control, LevelEdit},
    editor::{Editor, Status},
};
use byakko_core::session::{Activity, Status as SessionStatus};
pub(crate) use host::HostInput;
use iced::{
    Element, Fill,
    widget::{button, column, pick_list, row, scrollable, slider, text},
};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Panel {
    #[default]
    Onboard,
    Host,
    PerKey,
}

#[derive(Clone, Debug)]
pub(super) enum Message {
    PickerInteraction(crate::color_picker::Interaction),
    Panel(Panel),
    #[cfg(test)]
    Read,
    #[cfg(test)]
    Apply,
    #[cfg(test)]
    Revert,
    #[cfg(test)]
    Edit(Edit),
    Live(Edit),
    Retry,
    SelectHost(String),
    EditHost(Edit),
    StartHost(String),
    StopHost,
    Screen(screen::Message),
}

impl Desktop {
    pub(super) fn update_lighting(&mut self, message: Message) -> iced::Task<AppMessage> {
        if let Message::Screen(message) = message {
            let editable = !self.busy();
            return self.screen_capture.update(message, editable);
        }
        if let Message::Live(edit) = message {
            if let Edit::Effect(effect) = &edit {
                self.lighting_panel = if self
                    .session
                    .picture()
                    .and_then(|editor| editor.capabilities().lighting_effect.as_ref())
                    == Some(effect)
                {
                    Panel::PerKey
                } else {
                    Panel::Onboard
                };
            }
            if self.host.is_some() || self.session.status() == &SessionStatus::Disconnected {
                return iced::Task::none();
            }
            let Some(editor) = self.session.lighting() else {
                return iced::Task::none();
            };
            let mut proposed = self.live_lighting.clone();
            proposed.push(
                edit,
                std::time::Instant::now(),
                self.config.short_edit_delay,
            );
            match proposed.projected(editor) {
                Ok(Some(_)) => {
                    self.live_lighting = proposed;
                    self.notice = None;
                    if !self.busy()
                        && self.session.lighting().is_some_and(|editor| {
                            matches!(
                                editor.status(),
                                Status::Unverified { .. } | Status::Conflict { .. }
                            )
                        })
                    {
                        self.reload_lighting_intent();
                    } else {
                        self.flush_live_lighting();
                    }
                }
                Ok(None) => self.notice = Some("Lighting is not editable".into()),
                Err(reason) => self.notice = Some(reason),
            }
            return iced::Task::none();
        }
        match message {
            Message::PickerInteraction(event) => {
                use crate::color_picker::{Gesture, Interaction};
                match event {
                    Interaction::Started => self.picker_gesture = Gesture::Dragging,
                    Interaction::Finished => self.picker_gesture = Gesture::Idle,
                    Interaction::Moved => {}
                }
                let now = std::time::Instant::now();
                self.live_lighting
                    .postpone(now, self.config.short_edit_delay);
                self.live_picture.postpone(now, self.config.auto_save_delay);
            }
            Message::Panel(panel) => {
                self.lighting_panel = panel;
                if panel == Panel::PerKey
                    && let Some(effect) = self
                        .session
                        .picture()
                        .and_then(|editor| editor.capabilities().lighting_effect.clone())
                {
                    return self.update_lighting(Message::Live(Edit::Effect(effect)));
                }
            }
            Message::StopHost => self.stop_host(),
            Message::StartHost(mode_id) if !self.busy() => self.start_host(mode_id),
            Message::SelectHost(id) => match self.session.select_host_mode(&id) {
                Ok(()) => {
                    self.lighting_panel = Panel::Host;
                    self.live_lighting.clear();
                    self.notice = None;
                }
                Err(reason) => self.notice = Some(reason),
            },
            _ if self.busy() => (),
            #[cfg(test)]
            Message::Read => {
                let request = self.session.request_lighting_read();
                self.submit(request);
            }
            Message::Retry if !self.busy() => {
                if self.refresh_connection() {
                    self.live_lighting.retry();
                    self.reload_lighting_intent();
                }
            }
            #[cfg(test)]
            Message::Apply => {
                let request = self.session.request_lighting_apply();
                self.submit(request);
            }
            #[cfg(test)]
            Message::Revert => self.notice = self.session.revert_lighting().err(),
            #[cfg(test)]
            Message::Edit(edit) => self.notice = self.session.edit_lighting(edit).err(),
            Message::EditHost(edit) => self.notice = self.session.edit_host_setting(edit).err(),
            Message::StartHost(_) => {}
            Message::Screen(_) => unreachable!(),
            Message::Live(_) => unreachable!(),
            Message::Retry => {}
        }
        iced::Task::none()
    }

    pub(crate) fn flush_live_lighting(&mut self) -> bool {
        if self.picker_gesture == crate::color_picker::Gesture::Dragging
            || self.busy()
            || !self.live_lighting.ready(std::time::Instant::now())
        {
            return false;
        }
        if self.session.status() == &SessionStatus::Disconnected {
            self.live_lighting.block();
            return false;
        }
        let Some(editor) = self.session.lighting() else {
            self.live_lighting.clear();
            return false;
        };
        if editor.status() != &Status::Ready {
            if matches!(
                editor.status(),
                Status::Unverified {
                    problem: byakko_core::session::Problem::ReadRequired
                }
            ) {
                self.reload_lighting_intent();
                return true;
            }
            self.live_lighting.block();
            return false;
        }
        let desired = match self.live_lighting.projected(editor) {
            Ok(Some(setting)) => setting,
            Ok(None) => return false,
            Err(reason) => {
                self.notice = Some(reason);
                self.live_lighting.block();
                return false;
            }
        };
        if editor.draft() == Some(&desired) && !editor.dirty() {
            self.live_lighting.clear();
            return false;
        }
        if let Err(reason) = self.session.stage_lighting(desired) {
            self.notice = Some(reason);
            self.live_lighting.block();
            return false;
        }
        let command = match self.session.request_lighting_apply() {
            Ok(command) => command,
            Err(reason) => {
                self.notice = Some(reason);
                self.live_lighting.block();
                return false;
            }
        };
        if self.lighting_panel == Panel::PerKey
            && let byakko_core::session::Command::ApplyLighting {
                generation,
                operation,
                ..
            } = &command
        {
            self.picture_activation = Some((*generation, *operation));
        }
        self.submit(Ok(command));
        true
    }

    fn reload_lighting_intent(&mut self) {
        if let Err(reason) = self.session.revert_lighting() {
            self.notice = Some(reason);
            self.live_lighting.block();
            return;
        }
        match self.session.request_lighting_read() {
            Ok(command) => self.submit(Ok(command)),
            Err(reason) => {
                self.notice = Some(reason);
                self.live_lighting.block();
            }
        }
    }
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    column![mode_selector(app), mode_controls(app)]
        .spacing(app.ui.spacing.m)
        .height(Fill)
        .into()
}

fn mode_controls(app: &Desktop) -> Element<'_, AppMessage> {
    if shows_per_key(app) {
        return super::picture::view(app);
    }
    let Some(editor) = app.session.lighting() else {
        return text("Lighting is unavailable on this device").into();
    };
    let editable = app.host.is_none()
        && app.session.status() != &SessionStatus::Disconnected
        && editor.draft().is_some()
        && matches!(editor.status(), Status::Ready | Status::Unverified { .. });
    let style = &app.ui;
    let retry = (app.session.status() != &SessionStatus::Disconnected
        && (app.live_lighting.blocked()
            || matches!(
                editor.status(),
                Status::Unverified { .. } | Status::Conflict { .. }
            )))
    .then(|| {
        button("Reload & retry")
            .on_press_maybe((!app.busy()).then_some(AppMessage::Lighting(Message::Retry)))
    });
    let mut feedback = row![text(status(app, editor))].spacing(style.spacing.s);
    if let Some(retry) = retry {
        feedback = feedback.push(retry);
    }
    let mut content = column![feedback].spacing(style.spacing.s).height(Fill);
    if app.lighting_panel == Panel::Host && !editor.capabilities().host_modes.is_empty() {
        return content.push(host_controls(app, editor)).into();
    }
    let Some(draft) = editor.draft() else {
        if let Some(snapshot) = editor.baseline() {
            match &snapshot.content {
                Content::HostActive { .. } => {}
                Content::Opaque { reason } => content = content.push(text(reason)),
                Content::Editable(_) => {}
            }
        }
        return content.into();
    };
    let projected_setting = app.live_lighting.projected(editor).ok().flatten();
    let shown = projected_setting.as_ref().unwrap_or(draft);
    let projected = match controls::controls(editor.capabilities(), shown) {
        Ok(projected) => projected,
        Err(reason) => return content.push(text(reason)).into(),
    };

    let settings: Vec<_> = projected
        .settings
        .into_iter()
        .filter(|control| {
            !matches!(
                control,
                Control::Level {
                    edit: LevelEdit::Channel(_),
                    ..
                }
            )
        })
        .collect();
    let swatches = match shown.color {
        Some(Color::Rgb(rgb)) => crate::color_picker::view(
            style,
            rgb,
            format!("onboard:{}", shown.effect),
            |event| AppMessage::Lighting(Message::PickerInteraction(event)),
            editable
                .then_some(|rgb| AppMessage::Lighting(Message::Live(Edit::Color(Color::Rgb(rgb))))),
        ),
        _ => column![].into(),
    };
    content
        .push(
            scrollable(
                row![
                    swatches,
                    setting_controls(style, &settings, editable, Message::Live)
                ]
                .spacing(style.spacing.m),
            )
            .height(Fill),
        )
        .into()
}

pub(super) fn shows_per_key(app: &Desktop) -> bool {
    if app.lighting_panel == Panel::Host {
        return false;
    }
    if app.lighting_panel == Panel::PerKey
        || (app.session.lighting().is_none() && app.session.picture().is_some())
    {
        return true;
    }
    let Some(effect) = app
        .session
        .picture()
        .and_then(|editor| editor.capabilities().lighting_effect.as_ref())
    else {
        return false;
    };
    app.session.lighting().is_some_and(|editor| {
        app.live_lighting
            .projected(editor)
            .ok()
            .flatten()
            .or_else(|| editor.draft().cloned())
            .is_some_and(|setting| &setting.effect == effect)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ModeKind {
    PerKey,
    Onboard(String),
    Host(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModeChoice {
    kind: ModeKind,
    label: String,
}

impl fmt::Display for ModeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn mode_choices(app: &Desktop) -> (Vec<ModeChoice>, Option<ModeChoice>) {
    let mut choices = Vec::new();
    if app.session.picture().is_some() {
        choices.push(ModeChoice {
            kind: ModeKind::PerKey,
            label: "Per-key colors".into(),
        });
    }
    if let Some(editor) = app.session.lighting() {
        let projected = app.live_lighting.projected(editor).ok().flatten();
        let shown = projected.as_ref().or(editor.draft());
        let picture_effect = app
            .session
            .picture()
            .and_then(|editor| editor.capabilities().lighting_effect.as_ref());
        for choice in controls::effect_choices(
            editor.capabilities(),
            shown.map(|setting| setting.effect.as_str()),
        ) {
            if matches!(&choice.edit, Edit::Effect(id) if Some(id) == picture_effect) {
                continue;
            }
            let Edit::Effect(id) = choice.edit else {
                continue;
            };
            choices.push(ModeChoice {
                kind: ModeKind::Onboard(id),
                label: choice.label,
            });
        }
        for mode in &editor.capabilities().host_modes {
            choices.push(ModeChoice {
                kind: ModeKind::Host(mode.id.clone()),
                label: mode.label.clone(),
            });
        }
    }
    let selected_kind = match app.lighting_panel {
        _ if shows_per_key(app) => Some(ModeKind::PerKey),
        Panel::Onboard => app.session.lighting().and_then(|editor| {
            app.live_lighting
                .projected(editor)
                .ok()
                .flatten()
                .or_else(|| editor.draft().cloned())
                .map(|setting| ModeKind::Onboard(setting.effect))
        }),
        Panel::Host => app
            .session
            .host_draft()
            .map(|draft| ModeKind::Host(draft.mode_id.clone())),
        Panel::PerKey => Some(ModeKind::PerKey),
    };
    let selected = choices
        .iter()
        .find(|choice| Some(&choice.kind) == selected_kind.as_ref())
        .cloned();
    (choices, selected)
}

fn mode_selector(app: &Desktop) -> Element<'_, AppMessage> {
    let (mut choices, selected) = mode_choices(app);
    if app.host.is_some() || app.session.status() == &SessionStatus::Disconnected {
        choices.retain(|choice| Some(choice) == selected.as_ref());
    }
    column![
        text("Lighting mode"),
        crate::clipped_dropdown::clipped(
            pick_list(choices, selected, |choice: ModeChoice| {
                AppMessage::Lighting(match choice.kind {
                    ModeKind::PerKey => Message::Panel(Panel::PerKey),
                    ModeKind::Onboard(id) => Message::Live(Edit::Effect(id)),
                    ModeKind::Host(id) => Message::SelectHost(id),
                })
            })
            .placeholder("Select lighting mode")
            .width(app.ui.fields.regular)
            .into()
        ),
    ]
    .spacing(app.ui.spacing.xs)
    .into()
}
fn host_controls(app: &Desktop, editor: &Editor) -> Element<'static, AppMessage> {
    let selected_id = app.session.host_draft().map(|draft| draft.mode_id.as_str());
    let selected = selected_id
        .and_then(|id| {
            editor
                .capabilities()
                .host_modes
                .iter()
                .find(|mode| mode.id == id)
        })
        .or_else(|| editor.capabilities().host_modes.first())
        .expect("nonempty host modes");
    let can_start = !app.busy()
        && app.session.status() == &SessionStatus::Ready
        && editor.status() == &Status::Ready
        && editor
            .baseline()
            .is_some_and(|snapshot| matches!(snapshot.content, Content::Editable(_)))
        && !editor.dirty();
    let selected_setting = selected.parameters.as_ref().map(|parameters| {
        app.session
            .host_draft()
            .filter(|draft| draft.mode_id == selected.id)
            .and_then(|draft| draft.setting.as_ref())
            .unwrap_or(&parameters.default)
    });
    let settings = selected.parameters.as_ref().map(|parameters| {
        controls::parameter_controls(
            &parameters.schema,
            selected_setting.expect("selected parameter setting"),
        )
    });
    let host_color = match selected_setting.and_then(|setting| setting.color.as_ref()) {
        Some(Color::Rgb(rgb)) => crate::color_picker::view(
            &app.ui,
            *rgb,
            format!("host:{}", selected.id),
            |event| AppMessage::Lighting(Message::PickerInteraction(event)),
            (!app.busy()).then_some(|rgb| {
                AppMessage::Lighting(Message::EditHost(Edit::Color(Color::Rgb(rgb))))
            }),
        ),
        _ => column![].into(),
    };
    let parameters: Element<'static, AppMessage> = match settings {
        Some(Ok(projected)) => row![
            host_color,
            setting_controls(&app.ui, &projected, !app.busy(), Message::EditHost)
        ]
        .spacing(app.ui.spacing.s)
        .into(),
        Some(Err(reason)) => text(reason).into(),
        None => column![].into(),
    };
    let capture: Element<'static, AppMessage> =
        if selected.source == byakko_core::lighting::HostSource::ScreenAverage {
            app.screen_capture.view(&app.ui, !app.busy())
        } else {
            column![].into()
        };
    let start = button("Start").on_press_maybe(can_start.then_some(AppMessage::Lighting(
        Message::StartHost(selected.id.clone()),
    )));
    let stop = button("Stop & restore").on_press_maybe(
        app.host
            .as_ref()
            .map(|_| AppMessage::Lighting(Message::StopHost)),
    );
    let actions = row![start, stop].spacing(app.ui.spacing.s);
    if selected.source == byakko_core::lighting::HostSource::ScreenAverage {
        return row![
            iced::widget::Space::new().width(
                app.ui.color_picker_size.0 + app.ui.fields.compact as f32 + app.ui.spacing.s as f32
            ),
            column![actions, capture].spacing(app.ui.spacing.s),
        ]
        .spacing(app.ui.spacing.m)
        .into();
    }
    column![actions, parameters]
        .spacing(app.ui.spacing.s)
        .into()
}

fn status(app: &Desktop, editor: &Editor) -> String {
    if matches!(app.session.activity(), Activity::ApplyLighting { .. }) {
        return "Updating lighting…".into();
    }
    if app.live_lighting.has_pending() {
        return "Updating lighting…".into();
    }
    if app.live_lighting.blocked() && app.live_lighting.has_queued() {
        return format!(
            "{} · requested changes kept; reload before retrying",
            match editor.status() {
                Status::Unverified { problem } => super::view::problem_label(problem),
                _ => "Lighting update paused".into(),
            }
        );
    }
    if app.busy() {
        return super::view::status(app);
    }
    match editor.status() {
        Status::Unloaded => "Loading lighting…".into(),
        Status::Ready
            if matches!(
                editor.baseline().map(|snapshot| &snapshot.content),
                Some(Content::HostActive { .. })
            ) && editor.draft().is_none() =>
        {
            "Host lighting is active · select an onboard effect to return to local lighting".into()
        }
        Status::Ready => String::new(),
        Status::Conflict { .. } => "Lighting changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => super::view::problem_label(problem),
    }
}

fn setting_controls(
    style: &UiStyle,
    source: &[Control],
    editable: bool,
    message: fn(Edit) -> Message,
) -> Element<'static, AppMessage> {
    column(
        source
            .iter()
            .filter(|control| {
                !matches!(
                    control,
                    Control::Level {
                        edit: LevelEdit::Channel(_),
                        ..
                    }
                )
            })
            .cloned()
            .map(|control| match control {
                Control::Choices { label, choices } => row![
                    text(label).width(style.fields.compact),
                    row(choices.into_iter().map(|choice| {
                        panels::selectable_button(
                            style,
                            choice.label,
                            choice.selected,
                            editable.then_some(AppMessage::Lighting(message(choice.edit))),
                        )
                    }))
                    .spacing(style.spacing.xs)
                    .width(style.fields.regular)
                    .wrap(),
                ]
                .spacing(style.spacing.s)
                .align_y(iced::Alignment::Center)
                .into(),
                Control::Level {
                    label,
                    range,
                    value,
                    edit,
                } => {
                    let mut controls = row![
                        text(label).width(style.fields.compact),
                        text(value).width(style.color_hue_width)
                    ]
                    .spacing(style.spacing.s)
                    .align_y(iced::Alignment::Center);
                    if editable && range.start() != range.end() {
                        controls = controls.push(
                            slider(range, value, move |value| {
                                AppMessage::Lighting(message(
                                    edit.edit(value).expect("projected range"),
                                ))
                            })
                            .step(1u16)
                            .width(style.fields.regular),
                        );
                    }
                    controls.into()
                }
            }),
    )
    .spacing(style.spacing.s)
    .into()
}
