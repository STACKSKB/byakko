//! Per-key paint and local Looks gallery inside the lighting workspace.
use crate::{
    form::picture::{Form, Message, Mode},
    widget::{
        color_picker,
        focus_button::button,
        keyboard,
        panels::{self, UiStyle},
    },
};
use byakko_core::{
    editor::{Editor, Status, picture::PictureRules},
    model::{keymap::Descriptor, picture::Content},
};
use byakko_devices::nia87::looks::{ALL, Look};
use iced::{
    Alignment, Background, Border, Color, Element, Fill, Length, Theme,
    widget::{checkbox, column, container, row, space, text},
};
use std::collections::{BTreeMap, BTreeSet};

pub fn board<'a>(
    form: &'a Form,
    descriptor: &'a Descriptor,
    editor: &'a Editor<PictureRules>,
    editable: bool,
    style: &'a UiStyle,
    labels: BTreeMap<String, keyboard::BoardLabel>,
) -> Element<'a, Message> {
    let editable = editable && editor.status() == &Status::Ready && editor.draft().is_some();
    let advertised = &editor.capabilities().keys;
    keyboard::colored_view_with_labels(
        style,
        descriptor.keys.iter().filter(|key| key.visible).collect(),
        form.selected.clone(),
        form.display_colors(editor),
        labels,
        move |key| {
            (editable && advertised.contains(&key.id)).then(|| {
                if form.mode() == Mode::Paint {
                    Message::Paint(key.id.clone())
                } else {
                    Message::Select(key.id.clone())
                }
            })
        },
    )
}

fn swatch<Message>(rgb: [u8; 3], width: f32, height: f32) -> Element<'static, Message>
where
    Message: 'static,
{
    container(space())
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgb8(rgb[0], rgb[1], rgb[2]))),
            border: Border {
                width: 1.0,
                color: theme.extended_palette().background.strong.color,
                radius: 3.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn look_tile<'a>(
    look: Look,
    brush: [u8; 3],
    active: bool,
    enabled: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let colors = look.colors(brush);
    let unique: Vec<_> = colors
        .values()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(7)
        .collect();
    let mut sample = row![].spacing(2);
    for rgb in unique {
        sample = sample.push(swatch::<Message>(rgb, 18.0, 15.0));
    }
    let label = look.name();
    let content =
        column![sample, text(label).size(style.type_scale.body)].spacing(style.spacing.xs);
    button(content)
        .width(Fill)
        .padding(style.spacing.s as u16)
        .on_press_maybe(enabled.then_some(Message::Preview(look)))
        .style(move |theme, status| {
            if active {
                iced::widget::button::primary(theme, status)
            } else {
                iced::widget::button::secondary(theme, status)
            }
        })
        .into()
}

pub fn view<'a>(
    form: &'a Form,
    descriptor: &'a Descriptor,
    editor: &'a Editor<PictureRules>,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    if !matches!(editor.status(), Status::Ready) {
        let mut content =
            column![button("Read colors").on_press_maybe(idle.then_some(Message::Read))]
                .spacing(style.spacing.s);
        if editor.dirty() {
            content = content
                .push(button("Revert edits").on_press_maybe(idle.then_some(Message::Revert)));
        }
        return content.into();
    }
    let editable = editable && editor.draft().is_some();
    let can_preview = editable
        && editor.submitted().is_none()
        && editor.capabilities().backend_id == "nia87"
        && ALL.iter().all(|look| {
            look.colors([255, 32, 96])
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
                == editor.capabilities().keys.iter().cloned().collect()
        });
    let mut content = column![].spacing(style.spacing.m).width(Fill);

    if let Some(selected) = form.selected.as_ref() {
        let label = descriptor
            .keys
            .iter()
            .find(|key| &key.id == selected)
            .map_or(selected.as_str(), |key| key.label.as_str());
        let mut selection = row![text(label)]
            .spacing(style.spacing.s)
            .align_y(Alignment::Center);
        if let Some((device, shown)) = form.selected_colors(editor) {
            selection = selection
                .push(text("Device"))
                .push(swatch::<Message>(device, 18.0, 18.0));
            if form.preview_look().is_some() && device != shown {
                selection = selection
                    .push(text("Preview"))
                    .push(swatch::<Message>(shown, 18.0, 18.0));
            }
        }
        content = content.push(selection);
    } else {
        content = content.push(text("Select a key"));
    }

    content = content.push(
        row![
            panels::selectable_button(
                style,
                "Inspect",
                form.mode() == Mode::Inspect,
                editable.then_some(Message::Mode(Mode::Inspect))
            ),
            panels::selectable_button(
                style,
                "Paint keys",
                form.mode() == Mode::Paint,
                editable.then_some(Message::Mode(Mode::Paint))
            ),
        ]
        .spacing(style.spacing.s),
    );
    if let Some(rgb) = form.color(editor) {
        content = content.push(color_picker::view(
            style,
            rgb,
            "picture-brush".into(),
            Message::Picker,
            editable.then_some(Message::Brush),
        ));
    } else if let Some(Content::Opaque { reason }) = editor.baseline().map(|s| &s.content) {
        content = content.push(text(reason));
    }

    if editor.capabilities().backend_id == "nia87" {
        content = content.push(text("Looks").size(style.type_scale.section_title));
        for pair in ALL.chunks(2) {
            let mut tiles = row![].spacing(style.spacing.s).width(Fill);
            for look in pair {
                tiles = tiles.push(look_tile(
                    *look,
                    form.color(editor).unwrap_or([255, 32, 96]),
                    form.preview_look() == Some(*look),
                    can_preview,
                    style,
                ));
            }
            content = content.push(tiles);
        }
        if form.painted_count() > 0 {
            content = content.push(
                checkbox(form.replacing_painted())
                    .label(format!("Replace painted keys ({})", form.painted_count()))
                    .on_toggle_maybe(can_preview.then_some(Message::ReplacePainted)),
            );
        }
        if form.preview_look().is_some() {
            let applying = form.applying_preview();
            let preview_target = form.target(editor);
            content = content.push(
                row![
                    button("Revert preview")
                        .on_press_maybe((!applying).then_some(Message::RevertPreview)),
                    button("Apply look").on_press_maybe(
                        (can_preview && idle && !applying && preview_target.is_ok())
                            .then_some(Message::ApplyPreview)
                    ),
                ]
                .spacing(style.spacing.s),
            );
            content = content.push(text(if applying {
                "Applying look…".to_owned()
            } else if let Err(reason) = preview_target {
                reason
            } else {
                "Preview only · Device unchanged".to_owned()
            }));
        }
    }
    content.into()
}
