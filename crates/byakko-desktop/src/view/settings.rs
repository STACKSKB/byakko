//! Compact settings grid; each accepted slider or toggle edit uses the core draft.
use crate::widget::focus_button::button;
use crate::{
    form::settings::Message,
    widget::{
        focus_control,
        panels::{self, UiStyle},
    },
};
use byakko_core::{
    editor::{Editor, Feature, Status, settings::SettingsRules},
    model::settings::{Content, Edit, Field, Kind, Value},
};
use iced::{
    Alignment, Element, Fill,
    widget::{checkbox, column, container, row, slider, text},
};

pub fn view<'a>(
    editor: &'a Editor<SettingsRules>,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let editable = editable && editor.status() == &Status::Ready && editor.draft().is_some();
    let changes = editor.changes();
    let submitted = editor.submitted().and_then(|submitted| {
        let baseline = editor.baseline().and_then(SettingsRules::value)?;
        submitted
            .iter()
            .find(|(id, value)| baseline.get(*id) != Some(*value))
            .map(|(id, _)| id.as_str())
    });
    let pending = submitted.or_else(|| changes.first().map(|edit| edit.id.as_str()));
    let mut content = column![].spacing(style.spacing.s);
    if !matches!(editor.status(), Status::Ready) {
        let mut actions = row![].spacing(style.spacing.s);
        if editor.dirty() {
            actions =
                actions.push(button("Revert").on_press_maybe(idle.then_some(Message::Revert)));
        }
        actions = actions.push(
            button(if editor.status() == &Status::Unloaded {
                "Read settings"
            } else {
                "Reload & retry"
            })
            .on_press_maybe(idle.then_some(Message::Read)),
        );
        content = content.push(actions);
    }
    if let Some(values) = editor.draft() {
        let mut grid = column![].spacing(style.spacing.xs).width(Fill);
        for field in &editor.capabilities().fields {
            let can_edit = editable && pending.is_none_or(|id| id == field.id);
            let Some(value) = values.get(&field.id) else {
                continue;
            };
            grid = grid.push(
                container(
                    row![
                        text(&field.label).width(style.fields.regular),
                        control(style, field, value, can_edit)
                    ]
                    .spacing(style.spacing.s)
                    .align_y(Alignment::Center),
                )
                .padding(style.spacing.xs as u16)
                .width(Fill),
            );
        }
        content = content.push(
            panels::vertical_scroll(style, grid)
                .width(Fill)
                .height(Fill),
        );
    } else if let Some(Content::Opaque { reason }) =
        editor.baseline().map(|snapshot| &snapshot.content)
    {
        content = content.push(text(reason));
    }
    content.height(Fill).into()
}

fn control<'a>(
    style: &'a UiStyle,
    field: &'a Field,
    value: &Value,
    editable: bool,
) -> Element<'a, Message> {
    match (&field.kind, value) {
        (Kind::Toggle, Value::Toggle(current)) => {
            let id = field.id.clone();
            focus_control::toggle(
                checkbox(*current)
                    .label(if *current { "Enabled" } else { "Disabled" })
                    .on_toggle_maybe(editable.then_some(move |enabled| {
                        Message::Edit(Edit {
                            id: id.clone(),
                            value: Value::Toggle(enabled),
                        })
                    })),
                Message::Edit(Edit {
                    id: field.id.clone(),
                    value: Value::Toggle(!*current),
                }),
                editable,
            )
        }
        (
            Kind::Number {
                min,
                max,
                step,
                disabled_zero,
                unit,
            },
            Value::Number(current),
        ) => {
            let shown = if *disabled_zero && *current == 0 {
                "Disabled".into()
            } else {
                format!("{current} {unit}")
            };
            let mut controls = row![text(shown).width(style.fields.compact)]
                .spacing(style.spacing.s)
                .align_y(Alignment::Center);
            if editable {
                let id = field.id.clone();
                let max_value = u32::from(*max);
                let disabled_position = max_value + u32::from(*step);
                let range_end = if *disabled_zero {
                    disabled_position
                } else {
                    max_value
                };
                let position = if *disabled_zero && *current == 0 {
                    disabled_position
                } else {
                    u32::from(*current)
                };
                let slider = slider(u32::from(*min)..=range_end, position, move |number| {
                    Message::Edit(Edit {
                        id: id.clone(),
                        value: Value::Number(if number > max_value { 0 } else { number as u16 }),
                    })
                })
                .step(u32::from(*step))
                .width(style.fields.regular);
                controls = controls.push(focus_control::number(
                    slider,
                    position,
                    u32::from(*min),
                    range_end,
                    u32::from(*step),
                    true,
                    |number| {
                        Message::Edit(Edit {
                            id: field.id.clone(),
                            value: Value::Number(if number > max_value {
                                0
                            } else {
                                number as u16
                            }),
                        })
                    },
                ));
            }
            controls.into()
        }
        _ => text("Setting value does not match its capability").into(),
    }
}
