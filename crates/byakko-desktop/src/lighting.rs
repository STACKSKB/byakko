//! Render capability-projected lighting controls; device policy lives in core.
mod host;
pub(crate) mod screen;
use super::{Desktop, Message as AppMessage};
use crate::{
    control_widgets::{self, Choice},
    panels::{self, UiStyle},
};
use byakko_core::lighting::{
    Color, Content, Edit,
    controls::{self, ChoiceEdit, Control},
    editor::{Editor, Status},
};
use byakko_core::session::Status as SessionStatus;
pub(crate) use host::HostInput;
use iced::{
    Element, Fill, FillPortion,
    widget::{button, column, container, row, scrollable, text},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Panel {
    #[default]
    Onboard,
    Host,
}

#[derive(Clone, Debug)]
pub(super) enum Message {
    Panel(Panel),
    Read,
    Apply,
    Revert,
    Edit(Edit),
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
        match message {
            Message::Panel(panel) => self.lighting_panel = panel,
            Message::StopHost => self.stop_host(),
            Message::StartHost(mode_id) if !self.busy() => self.start_host(mode_id),
            _ if self.busy() => (),
            Message::Read => {
                let request = self.session.request_lighting_read();
                self.submit(request);
            }
            Message::Apply => {
                let request = self.session.request_lighting_apply();
                self.submit(request);
            }
            Message::Revert => self.notice = self.session.revert_lighting().err(),
            Message::Edit(edit) => self.notice = self.session.edit_lighting(edit).err(),
            Message::SelectHost(id) => self.notice = self.session.select_host_mode(&id).err(),
            Message::EditHost(edit) => self.notice = self.session.edit_host_setting(edit).err(),
            Message::StartHost(_) => {}
            Message::Screen(_) => unreachable!(),
        }
        iced::Task::none()
    }
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let Some(editor) = app.session.lighting() else {
        return text("Lighting is unavailable on this device").into();
    };
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let style = &app.ui;
    let selector = row![
        panels::selectable_button(
            style,
            "Onboard effects",
            app.lighting_panel == Panel::Onboard,
            Some(AppMessage::Lighting(Message::Panel(Panel::Onboard))),
        ),
        panels::selectable_button(
            style,
            "Host modes",
            app.lighting_panel == Panel::Host,
            (!editor.capabilities().host_modes.is_empty())
                .then_some(AppMessage::Lighting(Message::Panel(Panel::Host))),
        ),
    ]
    .spacing(style.spacing.s);
    let mut content = column![
        toolbar(app, editor, editable),
        selector,
        text(status(app, editor))
    ]
    .spacing(style.spacing.s)
    .height(Fill);
    if app.lighting_panel == Panel::Host && !editor.capabilities().host_modes.is_empty() {
        return content.push(host_controls(app, editor)).into();
    }
    let Some(draft) = editor.draft() else {
        if let Some(snapshot) = editor.baseline() {
            match &snapshot.content {
                Content::HostActive { .. } => {
                    content = content.push(panels::panel(
                        &app.ui,
                        "Choose an onboard effect",
                        scrollable(choice_buttons(
                            &app.ui,
                            &controls::effect_choices(editor.capabilities(), None),
                            !app.busy() && *editor.status() == Status::Ready,
                        ))
                        .height(Fill)
                        .into(),
                    ));
                }
                Content::Opaque { reason } => content = content.push(text(reason)),
                Content::Editable(_) => {}
            }
        }
        return content.into();
    };
    let projected = match controls::controls(editor.capabilities(), draft) {
        Ok(projected) => projected,
        Err(reason) => return content.push(text(reason)).into(),
    };
    let effects = projected.effects;
    let settings = projected.settings;
    let swatches = match draft.color {
        Some(Color::Rgb(rgb)) => control_widgets::color_presets(
            style,
            rgb,
            editable
                .then_some(|rgb| AppMessage::Lighting(Message::Edit(Edit::Color(Color::Rgb(rgb))))),
        ),
        _ => column![].into(),
    };
    let workbench = row![
        container(panels::panel(
            style,
            "Effects",
            scrollable(choice_buttons(style, &effects, editable))
                .height(Fill)
                .into(),
        ))
        .width(FillPortion(style.panes.sidebar)),
        container(panels::panel(
            style,
            "Selected effect · parameters",
            scrollable(
                column![
                    swatches,
                    setting_controls(style, &settings, editable, Message::Edit)
                ]
                .spacing(style.spacing.s)
            )
            .height(Fill)
            .into(),
        ))
        .width(FillPortion(style.panes.detail)),
    ]
    .spacing(style.spacing.m)
    .height(Fill);
    content.push(workbench).height(Fill).into()
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
    let modes = control_widgets::choices(
        &app.ui,
        "Mode",
        editor.capabilities().host_modes.iter().map(|mode| Choice {
            label: mode.label.clone(),
            selected: mode.id == selected.id,
            message: (!app.busy())
                .then_some(AppMessage::Lighting(Message::SelectHost(mode.id.clone()))),
        }),
    );
    let settings = selected.parameters.as_ref().map(|parameters| {
        let setting = app
            .session
            .host_draft()
            .filter(|draft| draft.mode_id == selected.id)
            .and_then(|draft| draft.setting.as_ref())
            .unwrap_or(&parameters.default);
        controls::parameter_controls(&parameters.schema, setting)
    });
    let parameters = match settings {
        Some(Ok(projected)) => {
            setting_controls(&app.ui, &projected, !app.busy(), Message::EditHost)
        }
        Some(Err(reason)) => text(reason).into(),
        None => text("This mode has no parameters").into(),
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
    column![
        row![start, stop].spacing(app.ui.spacing.m),
        scrollable(panels::panel(
            &app.ui,
            "Host lighting",
            column![modes, parameters, capture]
                .spacing(app.ui.spacing.l)
                .into(),
        ))
        .height(Fill),
    ]
    .spacing(app.ui.spacing.s)
    .height(Fill)
    .into()
}

fn toolbar<'a>(app: &Desktop, editor: &Editor, editable: bool) -> Element<'a, AppMessage> {
    control_widgets::transaction_toolbar(
        &app.ui,
        "Read lighting",
        (!app.busy()).then_some(AppMessage::Lighting(Message::Read)),
        (!app.busy() && editor.dirty()).then_some(AppMessage::Lighting(Message::Revert)),
        (editable && editor.dirty()).then_some(AppMessage::Lighting(Message::Apply)),
        if editor.dirty() {
            "Staged changes"
        } else {
            "No staged changes"
        },
    )
}

fn status(app: &Desktop, editor: &Editor) -> String {
    if app.busy() {
        return super::view::status(app);
    }
    match editor.status() {
        Status::Unloaded => "Read lighting to begin".into(),
        Status::Ready
            if matches!(
                editor.baseline().map(|snapshot| &snapshot.content),
                Some(Content::HostActive { .. })
            ) && editor.draft().is_none() =>
        {
            "Host lighting is active · select an onboard effect to return to local lighting".into()
        }
        Status::Ready => "Readback verified · edits are staged until applied".into(),
        Status::Conflict { .. } => "Lighting changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => super::view::problem_label(problem),
    }
}

fn choice_buttons(
    style: &UiStyle,
    source: &[ChoiceEdit],
    editable: bool,
) -> Element<'static, AppMessage> {
    control_widgets::choices(
        style,
        "Available",
        source.iter().cloned().map(|choice| Choice {
            label: choice.label,
            selected: choice.selected,
            message: editable.then_some(AppMessage::Lighting(Message::Edit(choice.edit))),
        }),
    )
}

fn setting_controls(
    style: &UiStyle,
    source: &[Control],
    editable: bool,
    message: fn(Edit) -> Message,
) -> Element<'static, AppMessage> {
    column(source.iter().cloned().map(|control| match control {
        Control::Choices { label, choices } => control_widgets::choices(
            style,
            label,
            choices.into_iter().map(|choice| Choice {
                label: choice.label,
                selected: choice.selected,
                message: editable.then_some(AppMessage::Lighting(message(choice.edit))),
            }),
        ),
        Control::Level {
            label,
            range,
            value,
            edit,
        } => control_widgets::level(
            style,
            label,
            range,
            value,
            editable.then_some(move |value| {
                AppMessage::Lighting(message(edit.edit(value).expect("projected range")))
            }),
        ),
    }))
    .spacing(style.spacing.l)
    .into()
}
