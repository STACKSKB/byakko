//! Capability-driven scalar settings; the view knows no firmware fields.
use super::{Desktop, Message as AppMessage};
use crate::panels;
use byakko_core::settings::{
    Content, Edit, Field, Kind, Value,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill, Length, Size,
    widget::{button, checkbox, column, container, responsive, row, scrollable, slider, text},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) enum Message {
    Read,
    Apply,
    Revert,
    Edit(Edit),
}

impl Desktop {
    pub(super) fn update_settings(&mut self, message: Message) {
        if self.busy() {
            return;
        }
        match message {
            Message::Read => {
                let request = self.session.request_settings_read();
                self.submit(request);
            }
            Message::Apply => {
                let request = self.session.request_setting_apply();
                self.submit(request);
            }
            Message::Revert => self.notice = self.session.revert_settings().err(),
            Message::Edit(edit) => self.notice = self.session.edit_setting(edit).err(),
        }
    }
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let Some(editor) = app.session.settings() else {
        return text("Settings are unavailable on this device").into();
    };
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let mut content = column![toolbar(app, editor, editable)].spacing(app.ui.spacing.s);
    if let Some(message) = status(app, editor) {
        content = content.push(text(message));
    }
    let Some(draft) = editor.draft() else {
        if let Some(snapshot) = editor.baseline()
            && let Content::Opaque { reason } = &snapshot.content
        {
            content = content.push(text(reason));
        }
        return content.into();
    };
    let fields = &editor.capabilities().fields;
    let staged = editor.changes().first().map(|change| change.id.clone());
    let style = &app.ui;
    let controls = responsive(move |size: Size| {
        let cell = style.fields.regular as f32;
        let gap = style.spacing.s as f32;
        let columns = if size.width >= cell * 3.0 + gap * 2.0 {
            3
        } else if size.width >= cell * 2.0 + gap {
            2
        } else {
            1
        };
        scrollable(settings_grid(
            style,
            fields,
            draft,
            editable,
            staged.as_deref(),
            columns,
        ))
        .height(Fill)
        .into()
    });
    content.push(controls).height(Fill).into()
}

fn settings_grid(
    style: &panels::UiStyle,
    fields: &[Field],
    draft: &BTreeMap<String, Value>,
    editable: bool,
    staged: Option<&str>,
    columns: usize,
) -> Element<'static, AppMessage> {
    let mut grid = column!().spacing(style.spacing.s).width(Fill);
    let mut pair = Vec::new();
    for field in fields {
        let Some(value) = draft.get(&field.id) else {
            continue;
        };
        let can_edit = editable && staged.is_none_or(|id| id == field.id);
        let card = container(panels::panel(
            style,
            field.label.clone(),
            control(style, field, value, can_edit),
        ))
        .width(Length::Fixed(style.fields.regular as f32));
        pair.push(card.into());
        if pair.len() == columns {
            grid = grid.push(
                container(row(std::mem::take(&mut pair)).spacing(style.spacing.s)).center_x(Fill),
            );
        }
    }
    if !pair.is_empty() {
        grid = grid.push(container(row(pair).spacing(style.spacing.s)).center_x(Fill));
    }
    grid.into()
}

fn toolbar<'a>(app: &Desktop, editor: &Editor, editable: bool) -> Element<'a, AppMessage> {
    let mut actions = row!().spacing(app.ui.spacing.s);
    if editor.dirty() {
        actions = actions
            .push(
                button("Apply setting")
                    .on_press_maybe(editable.then_some(AppMessage::Settings(Message::Apply))),
            )
            .push(
                button("Revert")
                    .on_press_maybe((!app.busy()).then_some(AppMessage::Settings(Message::Revert))),
            )
            .push(text("Apply or revert to change another setting"));
    } else if !matches!(editor.status(), Status::Ready) {
        actions = actions.push(
            button("Read settings")
                .on_press_maybe((!app.busy()).then_some(AppMessage::Settings(Message::Read))),
        );
    }
    actions.into()
}

fn status(app: &Desktop, editor: &Editor) -> Option<String> {
    if app.busy() {
        return Some(super::view::status(app));
    }
    match editor.status() {
        Status::Unloaded | Status::Ready => None,
        Status::Conflict { .. } => {
            Some("Settings changed on the device. Revert the draft, then read again.".into())
        }
        Status::Unverified { problem } => Some(super::view::problem_label(problem)),
    }
}

fn control(
    style: &panels::UiStyle,
    field: &Field,
    value: &Value,
    editable: bool,
) -> Element<'static, AppMessage> {
    match (&field.kind, value) {
        (Kind::Toggle, Value::Toggle(current)) => {
            let id = field.id.clone();
            checkbox(*current)
                .label(if *current { "Enabled" } else { "Disabled" })
                .on_toggle_maybe(editable.then_some(move |enabled| {
                    AppMessage::Settings(Message::Edit(Edit {
                        id: id.clone(),
                        value: Value::Toggle(enabled),
                    }))
                }))
                .into()
        }
        (
            Kind::Number {
                min,
                max,
                step,
                disabled_zero,
                ..
            },
            Value::Number(current),
        ) => {
            let mut controls = column![text(display_value(field, value))].spacing(style.spacing.xs);
            if editable {
                let id = field.id.clone();
                controls = controls.push(
                    slider(
                        *min..=*max,
                        if *current == 0 { *min } else { *current },
                        move |number| {
                            AppMessage::Settings(Message::Edit(Edit {
                                id: id.clone(),
                                value: Value::Number(number),
                            }))
                        },
                    )
                    .step(*step),
                );
            }
            if *disabled_zero {
                let id = field.id.clone();
                let next = if *current == 0 { *min } else { 0 };
                controls = controls.push(
                    button(if *current == 0 { "Enable" } else { "Disable" }).on_press_maybe(
                        editable.then_some(AppMessage::Settings(Message::Edit(Edit {
                            id,
                            value: Value::Number(next),
                        }))),
                    ),
                );
            }
            controls.into()
        }
        _ => text("Setting value does not match its capability").into(),
    }
}

fn display_value(field: &Field, value: &Value) -> String {
    match (&field.kind, value) {
        (Kind::Toggle, Value::Toggle(true)) => "Enabled".into(),
        (Kind::Toggle, Value::Toggle(false)) => "Disabled".into(),
        (
            Kind::Number {
                disabled_zero: true,
                ..
            },
            Value::Number(0),
        ) => "Disabled".into(),
        (Kind::Number { unit, .. }, Value::Number(value)) => format!("{value} {unit}"),
        _ => "Unknown value".into(),
    }
}
