//! Capability-driven scalar settings; the view knows no firmware fields.
use super::{Desktop, Message as AppMessage};
use crate::{control_widgets, panels};
use byakko_core::settings::{
    Content, Edit, Field, Kind, Value,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill, FillPortion, Size,
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
    let mut content = column![toolbar(app, editor, editable), text(status(app, editor))]
        .spacing(app.ui.spacing.m);
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
        .width(FillPortion(1));
        pair.push(card.into());
        if pair.len() == columns {
            grid = grid.push(
                row(std::mem::take(&mut pair))
                    .spacing(style.spacing.s)
                    .width(Fill),
            );
        }
    }
    if !pair.is_empty() {
        grid = grid.push(row(pair).spacing(style.spacing.s).width(Fill));
    }
    grid.into()
}

fn toolbar<'a>(app: &Desktop, editor: &Editor, editable: bool) -> Element<'a, AppMessage> {
    control_widgets::transaction_toolbar(
        &app.ui,
        "Read settings",
        (!app.busy()).then_some(AppMessage::Settings(Message::Read)),
        (!app.busy() && editor.dirty()).then_some(AppMessage::Settings(Message::Revert)),
        (editable && editor.dirty()).then_some(AppMessage::Settings(Message::Apply)),
        if editor.dirty() {
            "One setting staged"
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
        Status::Unloaded => "Read settings to begin".into(),
        Status::Ready => "Readback verified · one setting can be staged at a time".into(),
        Status::Conflict { .. } => "Settings changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => super::view::problem_label(problem),
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
