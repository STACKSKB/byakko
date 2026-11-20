//! Device-neutral projection of physical-key coordinates into an Iced board.
use crate::widget::panels::UiStyle;
use byakko_core::model::keymap::{Action, Bindings, Descriptor, PhysicalKey};
use iced::{
    Background, Color, Element, Fill, Length, Size,
    widget::{container, pin, responsive, space, stack, text, tooltip},
};
use std::collections::BTreeMap;

/// The visible assignment and its complete description, derived from the current draft.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardLabel {
    pub compact: String,
    pub full: String,
}

/// Project one layer of portable bindings onto physical key IDs. An absent draft
/// leaves the board's physical legends in place until a keymap is read.
pub fn labels_for_layer(
    descriptor: &Descriptor,
    draft: Option<&Bindings>,
    layer: &str,
    assignment_name: impl Fn(&Action) -> Option<String>,
) -> BTreeMap<String, BoardLabel> {
    let Some(bindings) = draft.and_then(|draft| draft.get(layer)) else {
        return BTreeMap::new();
    };
    bindings
        .iter()
        .map(|(key, action)| {
            let full = assignment_name(action)
                .or_else(|| {
                    descriptor
                        .actions
                        .iter()
                        .find(|choice| choice.action == *action)
                        .map(|choice| choice.label.clone())
                })
                .unwrap_or_else(|| fallback_action_label(descriptor, action));
            let compact = match action {
                Action::Opaque { .. } => {
                    if descriptor
                        .layers
                        .iter()
                        .any(|entry| entry.id == layer && entry.read_only_keys.contains(key))
                    {
                        "Onboard".into()
                    } else {
                        "Unknown".into()
                    }
                }
                _ => compact_action_label(&full),
            };
            (key.clone(), BoardLabel { compact, full })
        })
        .collect()
}

fn fallback_action_label(descriptor: &Descriptor, action: &Action) -> String {
    match action {
        Action::Key(usage) => descriptor
            .shortcuts
            .as_ref()
            .and_then(|caps| caps.keys.iter().find(|choice| choice.usage == *usage))
            .map_or_else(|| format!("Key {usage}"), |choice| choice.label.clone()),
        Action::Disabled => "Disabled".into(),
        Action::Macro { .. } => "Macro".into(),
        Action::Shortcut { modifiers, key } => {
            let Some(caps) = &descriptor.shortcuts else {
                return "Shortcut".into();
            };
            let names: Option<Vec<_>> = modifiers
                .iter()
                .map(|usage| caps.modifiers.iter().find(|choice| choice.usage == *usage))
                .collect();
            let key_name = caps.keys.iter().find(|choice| choice.usage == *key);
            match (names, key_name) {
                (Some(names), Some(key_name)) => format!(
                    "{}+{}",
                    names
                        .iter()
                        .map(|choice| choice.label.as_str())
                        .collect::<Vec<_>>()
                        .join("+"),
                    key_name.label
                ),
                _ => "Shortcut".into(),
            }
        }
        Action::Named { id } => id.clone(),
        Action::Opaque { label, .. } => label.clone(),
    }
}

fn compact_action_label(full: &str) -> String {
    let compact = match full {
        "Calculator" => "Calc",
        "Scroll Lock" => "ScrLk",
        "Print Screen" => "PrtSc",
        "Page Up" => "PgUp",
        "Page Down" => "PgDn",
        "Backspace" => "Bksp",
        "Delete" => "Del",
        "Insert" => "Ins",
        "Escape" => "Esc",
        "Display Brightness Up" => "Bright+",
        "Display Brightness Down" => "Bright−",
        "Volume Up" => "Vol+",
        "Volume Down" => "Vol−",
        "Previous Track" => "Prev",
        "Next Track" => "Next",
        "Play/Pause" => "Play",
        "Disabled" => "Off",
        _ => full,
    };
    compact.to_owned()
}

fn bounds(keys: &[&PhysicalKey]) -> (f32, f32) {
    keys.iter().fold((0.0, 0.0), |(width, height), key| {
        (width.max(key.x + key.width), height.max(key.y + key.height))
    })
}

/// Uses the descriptor's physical geometry. The complete keyboard scales inside
/// its reserved stage, so changing tabs or scrolling an inspector cannot move it.
pub fn view_with_labels<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    physical_tooltips: bool,
    labels: BTreeMap<String, BoardLabel>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    board_view(
        style,
        keys,
        selected,
        Appearance {
            colors: BTreeMap::new(),
            physical_tooltips,
        },
        labels,
        on_select,
    )
}

pub fn colored_view_with_labels<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    colors: BTreeMap<String, [u8; 3]>,
    labels: BTreeMap<String, BoardLabel>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    board_view(
        style,
        keys,
        selected,
        Appearance {
            colors,
            physical_tooltips: false,
        },
        labels,
        on_select,
    )
}

struct Appearance {
    colors: BTreeMap<String, [u8; 3]>,
    physical_tooltips: bool,
}

fn board_view<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    appearance: Appearance,
    labels: BTreeMap<String, BoardLabel>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    let (span, rise) = bounds(&keys);
    container(responsive(move |size: Size| {
        let unit = ((size.width - 72.0).max(1.0) / span.max(1.0))
            .min((size.height - 22.0).max(1.0) / rise.max(1.0))
            .min(style.board.max_unit)
            .max(1.0);
        let board_width = span * unit;
        let board_height = rise * unit;
        let base: Element<'_, Message> = space()
            .width(Length::Fixed(board_width))
            .height(Length::Fixed(board_height))
            .into();
        let board = keys.iter().fold(stack![base], |board, key| {
            let gap = style.board.key_gap.min(unit * 0.12);
            let width = (key.width * unit - gap).max(1.0);
            let height = (key.height * unit - gap).max(1.0);
            let assigned = labels.get(&key.id);
            let label = physical_legend(&key.label, unit);
            let selected_key = selected.as_deref() == Some(key.id.as_str());
            let physical = physical_cap(&key.label);
            let face = appearance
                .colors
                .get(&key.id)
                .map(|[r, g, b]| Color::from_rgb8(*r, *g, *b))
                .unwrap_or(physical.face);
            let ink = super::controls::readable(physical.ink, face);
            let skirt = mix(face, Color::BLACK, 0.19);
            let label_size = if label.chars().count() > 3 && key.width <= 1.0 {
                if unit < 30.0 { 6.0 } else { 8.0 }
            } else if unit < 28.0 {
                7.0
            } else if unit < 38.0 {
                9.0
            } else {
                style.board.key_label_size as f32
            };
            let key_button: Element<'_, Message> = crate::widget::focus_button::button(
                container(
                    text(label)
                        .size(label_size)
                        .width(Fill)
                        .height(Fill)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .center()
                        .color(ink),
                )
                .width(Fill)
                .height(Fill)
                .padding(if unit < 28.0 { 1 } else { 4 })
                .style(move |_| container::Style {
                    background: Some(Background::Color(face)),
                    border: iced::Border {
                        radius: 4.0.into(),
                        color: if selected_key {
                            Color::BLACK
                        } else {
                            mix(face, Color::BLACK, 0.35)
                        },
                        width: if selected_key { 2.0 } else { 1.0 },
                    },
                    ..Default::default()
                })
                .id(iced::widget::Id::from(format!("cap-{}", key.id))),
            )
            .width(width)
            .height(height)
            .padding(if unit < 28.0 { 1 } else { 3 })
            .on_press_maybe(on_select(key))
            .style(move |theme, status| {
                let mut visual = iced::widget::button::secondary(theme, status);
                visual.background = Some(Background::Color(skirt));
                visual.border.color = if selected_key { Color::BLACK } else { skirt };
                visual.border.width = if selected_key { 2.0 } else { 1.0 };
                visual.border.radius = 6.0.into();
                visual
            })
            .into();
            let key_button: Element<'_, Message> = if appearance.physical_tooltips {
                tooltip(
                    key_button,
                    container(
                        text(assigned.map_or_else(
                            || key.label.clone(),
                            |assignment| format!("{}: {}", key.label, assignment.full),
                        ))
                        .size(style.board.key_label_size)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    )
                    .max_width(style.fields.regular),
                    tooltip::Position::Bottom,
                )
                .padding(style.spacing.xs as f32)
                .gap(style.spacing.xs as f32)
                .style(|theme: &iced::Theme| {
                    let palette = theme.extended_palette();
                    container::Style {
                        background: Some(
                            iced::Color {
                                a: 1.0,
                                ..palette.background.strong.color
                            }
                            .into(),
                        ),
                        text_color: Some(palette.background.strong.text),
                        ..Default::default()
                    }
                })
                .into()
            } else {
                key_button
            };
            board.push(pin(key_button).x(key.x * unit).y(key.y * unit))
        });
        let board = if has_nia87_indicators(&keys) {
            (0..2).fold(board, |board, index| {
                let indicator = container(space())
                    .width(Length::Fixed((unit * 0.12).clamp(3.0, 5.0)))
                    .height(Length::Fixed((unit * 0.12).clamp(3.0, 5.0)))
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgb8(63, 68, 76))),
                        border: iced::Border {
                            radius: 1.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .id(iced::widget::Id::from(format!("indicator-{}", index + 1)));
                board.push(
                    pin(indicator)
                        .x((15.32 + index as f32 * 0.20) * unit)
                        .y(3.78 * unit),
                )
            })
        } else {
            board
        };
        container(
            container(
                board
                    .width(Length::Fixed(board_width))
                    .height(Length::Fixed(board_height)),
            )
            .padding((unit * 0.2).clamp(4.0, 11.0))
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(205, 205, 210))),
                border: iced::Border {
                    radius: 9.0.into(),
                    color: Color::from_rgb8(151, 151, 160),
                    width: 1.0,
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba8(0, 0, 0, 0.18),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 5.0,
                },
                ..Default::default()
            }),
        )
        .center_x(Fill)
        .center_y(Fill)
        .width(Fill)
        .height(Fill)
        .into()
    }))
    .height(Fill)
    .width(Fill)
    .into()
}

fn has_nia87_indicators(keys: &[&PhysicalKey]) -> bool {
    keys.len() == 87 && keys.iter().all(|key| key.id.starts_with("slot-"))
}

#[derive(Clone, Copy)]
struct PhysicalCap {
    face: Color,
    ink: Color,
}

fn physical_cap(label: &str) -> PhysicalCap {
    let lavender = matches!(
        label,
        "Esc"
            | "F5"
            | "F6"
            | "F7"
            | "F8"
            | "PrtSc"
            | "ScrLk"
            | "Pause"
            | "Backspace"
            | "Tab"
            | "Caps"
            | "LShift"
            | "RShift"
            | "Enter"
            | "LCtrl"
            | "RCtrl"
            | "LWin"
            | "LAlt"
            | "RAlt"
            | "Fn"
            | "Menu"
            | "Insert"
            | "Delete"
            | "Home"
            | "End"
            | "PgUp"
            | "PgDn"
            | "Up"
            | "Left"
            | "Down"
            | "Right"
    );
    if lavender {
        PhysicalCap {
            face: Color::from_rgb8(0x6d, 0x65, 0xa9),
            ink: Color::WHITE,
        }
    } else {
        PhysicalCap {
            face: Color::from_rgb8(0xdc, 0xdb, 0xe1),
            ink: Color::from_rgb8(54, 49, 105),
        }
    }
}

fn physical_legend(label: &str, unit: f32) -> String {
    if unit < 38.0 {
        let compact = match label {
            "Backspace" => Some("BKSP"),
            "Caps" => Some("CAPS"),
            "Home" => Some("HOME"),
            "PgUp" => Some("PG↑"),
            "PgDn" => Some("PG↓"),
            _ => None,
        };
        if let Some(compact) = compact {
            return compact.into();
        }
    }
    let shown = match label {
        "`" => "~\n`",
        "1" => "!\n1",
        "2" => "@\n2",
        "3" => "#\n3",
        "4" => "$\n4",
        "5" => "%\n5",
        "6" => "^\n6",
        "7" => "&\n7",
        "8" => "*\n8",
        "9" => "(\n9",
        "0" => ")\n0",
        "-" => "_\n-",
        "=" => "+\n=",
        "[" => "{\n[",
        "]" => "}\n]",
        "\\" => "|\n\\",
        ";" => ":\n;",
        "'" => "\"\n'",
        "," => "<\n,",
        "." => ">\n.",
        "/" => "?\n/",
        "LShift" | "RShift" => "SHIFT",
        "LCtrl" | "RCtrl" => "CTRL",
        "LWin" => "WIN",
        "LAlt" | "RAlt" => "ALT",
        "Caps" => "CAPS LOCK",
        "Backspace" => "BACKSPACE",
        "Delete" => "DEL",
        "Insert" => "INS",
        "PgUp" => "PGUP",
        "PgDn" => "PGDN",
        "PrtSc" => "PS",
        "ScrLk" => "SL",
        "Pause" => "PB",
        "Up" => "\u{2191}",
        "Down" => "\u{2193}",
        "Left" => "\u{2190}",
        "Right" => "\u{2192}",
        _ => return label.to_uppercase(),
    };
    shown.into()
}

fn mix(first: Color, second: Color, amount: f32) -> Color {
    Color::from_rgb(
        first.r * (1.0 - amount) + second.r * amount,
        first.g * (1.0 - amount) + second.g * amount,
        first.b * (1.0 - amount) + second.b * amount,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::keymap::{ActionCategory, ActionChoice, Layer};

    fn key(id: &str, x: f32, y: f32, width: f32) -> PhysicalKey {
        PhysicalKey {
            id: id.into(),
            label: id.into(),
            position: None,
            x,
            y,
            width,
            height: 1.0,
            visible: true,
            writable: true,
        }
    }

    #[test]
    fn descriptor_bounds_include_stagger_and_tall_keys() {
        let esc = key("esc", 0.0, 0.0, 1.0);
        let mut enter = key("enter", 13.0, 2.0, 2.0);
        enter.height = 2.0;
        let arrow = key("arrow", 17.5, 5.5, 1.0);
        assert_eq!(bounds(&[&enter, &arrow, &esc]), (18.5, 6.5));
    }

    #[test]
    fn stock_nia87_uses_physical_labels_for_thirty_one_lavender_caps() {
        let session = byakko_devices::nia87::application::session().unwrap();
        let visible: Vec<_> = session
            .descriptor()
            .keys
            .iter()
            .filter(|key| key.visible)
            .collect();
        assert_eq!(visible.len(), 87);
        assert_eq!(
            visible
                .iter()
                .filter(|key| physical_cap(&key.label).face == Color::from_rgb8(0x6d, 0x65, 0xa9))
                .count(),
            31
        );
        assert!(visible.iter().all(|key| key.id.starts_with("slot-")));
        assert!(has_nia87_indicators(&visible));
    }

    #[test]
    fn assigned_macro_names_follow_binding_metadata_and_local_renames() {
        let device = byakko_devices::memory::demo().unwrap();
        let session = device.session().unwrap();
        let caps = session.macros().unwrap().capabilities();
        let mut names = crate::form::files::Form::default();
        let binding = &caps.bindings[0];
        names
            .names
            .insert(binding.slot.clone(), "Copy address".into());
        let draft = Bindings::from([(
            "main".into(),
            BTreeMap::from([("key".into(), binding.action.clone())]),
        )]);
        let projected = |names: &crate::form::files::Form| {
            labels_for_layer(session.descriptor(), Some(&draft), "main", |action| {
                names.assignment_name(action, caps).map(str::to_owned)
            })
        };
        assert_eq!(projected(&names)["key"].full, "Copy address");
        assert_eq!(projected(&names)["key"].compact, "Copy address");
        names.rename(&binding.slot, "Paste".into());
        assert_eq!(projected(&names)["key"].full, "Paste");
        names.rename(&binding.slot, " ".into());
        assert_eq!(projected(&names)["key"].full, caps.slots[0].label);
        assert!(names.assignment_name(&Action::Disabled, caps).is_none());
    }

    #[test]
    fn displayed_actions_follow_the_selected_draft_layer() {
        let descriptor = Descriptor {
            backend_id: "test".into(),
            device_name: "Test".into(),
            keys: vec![key("Q", 0.0, 0.0, 1.0), key("Scroll Lock", 1.0, 0.0, 1.0)],
            layers: vec![Layer {
                id: "main".into(),
                label: "Main".into(),
                read_only_keys: vec![],
            }],
            actions: vec![
                ActionChoice {
                    label: "A".into(),
                    action: Action::Key(4),
                    category: ActionCategory::Alphanumeric,
                },
                ActionChoice {
                    label: "Calculator".into(),
                    action: Action::Named { id: "calc".into() },
                    category: ActionCategory::System,
                },
            ],
            shortcuts: None,
        };
        let draft = Bindings::from([
            (
                "main".into(),
                BTreeMap::from([
                    ("Q".into(), Action::Key(4)),
                    ("Scroll Lock".into(), Action::Named { id: "calc".into() }),
                ]),
            ),
            (
                "other".into(),
                BTreeMap::from([("Q".into(), Action::Disabled)]),
            ),
        ]);
        let main = labels_for_layer(&descriptor, Some(&draft), "main", |_| None);
        assert_eq!(
            main["Q"],
            BoardLabel {
                compact: "A".into(),
                full: "A".into()
            }
        );
        assert_eq!(
            main["Scroll Lock"],
            BoardLabel {
                compact: "Calc".into(),
                full: "Calculator".into()
            }
        );
        assert_eq!(
            labels_for_layer(&descriptor, Some(&draft), "other", |_| None)["Q"].compact,
            "Off"
        );
        assert!(labels_for_layer(&descriptor, None, "main", |_| None).is_empty());
    }

    #[test]
    fn unknown_actions_keep_bytes_in_description_and_show_reserved_onboard_positions() {
        let mut descriptor = Descriptor {
            backend_id: "test".into(),
            device_name: "Test".into(),
            keys: vec![key("one", 0.0, 0.0, 1.0)],
            layers: vec![],
            actions: vec![],
            shortcuts: None,
        };
        let draft = Bindings::from([(
            "main".into(),
            BTreeMap::from([(
                "one".into(),
                Action::Opaque {
                    backend_id: "test".into(),
                    data: vec![1],
                    label: "Factory-specific action".into(),
                },
            )]),
        )]);
        let label = &labels_for_layer(&descriptor, Some(&draft), "main", |_| None)["one"];
        assert_eq!(label.full, "Factory-specific action");
        assert_eq!(label.compact, "Unknown");
        descriptor.layers.push(Layer {
            id: "main".into(),
            label: "Main".into(),
            read_only_keys: vec!["one".into()],
        });
        let label = &labels_for_layer(&descriptor, Some(&draft), "main", |_| None)["one"];
        assert_eq!(label.compact, "Onboard");
        assert_eq!(label.full, "Factory-specific action");
    }
}
