//! Render capability-projected lighting controls; device policy lives in core.
mod host;
use super::{Desktop, Message as AppMessage};
use crate::{
    control_widgets::{self, Choice},
    panels::{self, UiStyle},
};
use byakko_core::lighting::{
    Content, Edit, HostSource,
    controls::{self, ChoiceEdit, Control},
    editor::{Editor, Status},
};
use byakko_core::session::Status as SessionStatus;
pub(crate) use host::HostScreen;
use iced::{
    Element, Fill,
    widget::{button, column, row, scrollable, text},
};

#[derive(Clone, Debug)]
pub(super) enum Message {
    Read,
    Apply,
    Revert,
    Edit(Edit),
    StartScreen(String),
    StopHost,
}

impl Desktop {
    pub(super) fn update_lighting(&mut self, message: Message) {
        match message {
            Message::StopHost => self.stop_host(),
            Message::StartScreen(mode_id) if !self.busy() => self.start_screen(mode_id),
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
            Message::StartScreen(_) => {}
        }
    }
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let Some(editor) = app.session.lighting() else {
        return text("Lighting is unavailable on this device").into();
    };
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let mut content = column![toolbar(app, editor, editable), text(status(app, editor))]
        .spacing(app.ui.spacing.m);
    if !editor.capabilities().host_modes.is_empty() {
        content = content.push(host_controls(app, editor));
    }
    let Some(draft) = editor.draft() else {
        if let Some(snapshot) = editor.baseline()
            && let Content::Opaque { reason } = &snapshot.content
        {
            content = content.push(text(reason));
        }
        return content.into();
    };
    let projected = match controls::controls(editor.capabilities(), draft) {
        Ok(projected) => projected,
        Err(reason) => return content.push(text(reason)).into(),
    };
    let style = &app.ui;
    let effects = projected.effects;
    let settings = projected.settings;
    let workbench = panels::split(
        style,
        move || {
            panels::panel(
                style,
                "Effects",
                scrollable(choice_buttons(style, &effects, editable))
                    .height(Fill)
                    .into(),
            )
        },
        move || {
            panels::panel(
                style,
                "Parameters",
                scrollable(setting_controls(style, &settings, editable))
                    .height(Fill)
                    .into(),
            )
        },
    );
    content.push(workbench).height(Fill).into()
}

fn host_controls(app: &Desktop, editor: &Editor) -> Element<'static, AppMessage> {
    let can_start = !app.busy()
        && app.session.status() == &SessionStatus::Ready
        && editor.status() == &Status::Ready
        && !editor.dirty();
    let starts = column(
        editor
            .capabilities()
            .host_modes
            .iter()
            .filter(|mode| mode.source == HostSource::ScreenAverage)
            .map(|mode| {
                button(text(format!("Start {}", mode.label)))
                    .on_press_maybe(
                        can_start
                            .then_some(AppMessage::Lighting(Message::StartScreen(mode.id.clone()))),
                    )
                    .into()
            }),
    );
    let stop = button("Stop & restore").on_press_maybe(
        app.screen
            .as_ref()
            .map(|_| AppMessage::Lighting(Message::StopHost)),
    );
    panels::panel(
        &app.ui,
        "Host lighting",
        row![starts, stop].spacing(app.ui.spacing.m).into(),
    )
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
) -> Element<'static, AppMessage> {
    column(source.iter().cloned().map(|control| match control {
        Control::Choices { label, choices } => control_widgets::choices(
            style,
            label,
            choices.into_iter().map(|choice| Choice {
                label: choice.label,
                selected: choice.selected,
                message: editable.then_some(AppMessage::Lighting(Message::Edit(choice.edit))),
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
                AppMessage::Lighting(Message::Edit(edit.edit(value).expect("projected range")))
            }),
        ),
    }))
    .spacing(style.spacing.l)
    .into()
}
