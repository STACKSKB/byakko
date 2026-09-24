//! Device-neutral projection of physical-key coordinates into an Iced board.
use crate::panels::{self, UiStyle};
use byakko_core::{Action, Descriptor, PhysicalKey, session::Bindings};
use iced::{
    Element, Fill, Length, Size,
    widget::{container, pin, responsive, scrollable, space, stack, text, tooltip},
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
) -> BTreeMap<String, BoardLabel> {
    let Some(bindings) = draft.and_then(|draft| draft.get(layer)) else {
        return BTreeMap::new();
    };
    bindings
        .iter()
        .map(|(key, action)| {
            let full = descriptor
                .actions
                .iter()
                .find(|choice| choice.action == *action)
                .map(|choice| choice.label.clone())
                .unwrap_or_else(|| fallback_action_label(descriptor, action));
            let compact = compact_action_label(&full);
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
        Action::Macro { slot, .. } => format!("Macro {slot}"),
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
        "Volume Up" => "Vol+",
        "Volume Down" => "Vol−",
        "Previous Track" => "Prev",
        "Next Track" => "Next",
        "Play/Pause" => "Play",
        "Disabled" => "Off",
        _ => full,
    };
    if compact.chars().count() <= 7 {
        compact.to_owned()
    } else {
        format!("{}…", compact.chars().take(6).collect::<String>())
    }
}

fn bounds(keys: &[&PhysicalKey]) -> (f32, f32) {
    keys.iter().fold((0.0, 0.0), |(width, height), key| {
        (width.max(key.x + key.width), height.max(key.y + key.height))
    })
}

/// Uses the descriptor's physical geometry. Narrow windows scroll horizontally;
/// normal windows scale up only to the style's maximum key size.
pub fn view_with_labels<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    labels: BTreeMap<String, BoardLabel>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    colored_view_with_labels(style, keys, selected, BTreeMap::new(), labels, on_select)
}

pub fn colored_view_with_labels<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    colors: BTreeMap<String, [u8; 3]>,
    labels: BTreeMap<String, BoardLabel>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    let (span, rise) = bounds(&keys);
    container(responsive(move |size: Size| {
        let unit = (size.width / span.max(1.0))
            .min(size.height / rise.max(1.0))
            .clamp(style.board.min_unit, style.board.max_unit);
        let board_width = span * unit;
        let board_height = rise * unit;
        let base: Element<'_, Message> = space()
            .width(Length::Fixed(board_width))
            .height(Length::Fixed(board_height))
            .into();
        let board = keys.iter().fold(stack![base], |board, key| {
            let width = (key.width * unit - style.board.key_gap).max(1.0);
            let height = (key.height * unit - style.board.key_gap).max(1.0);
            let assigned = labels.get(&key.id);
            let label = assigned.map_or(key.label.as_str(), |label| label.compact.as_str());
            let mut key_button = panels::selectable_button_with_size(
                style,
                label,
                selected.as_deref() == Some(key.id.as_str()),
                on_select(key),
                Some((width, height)),
            );
            if let Some(rgb) = colors.get(&key.id) {
                let [r, g, b] = *rgb;
                let selected = selected.as_deref() == Some(key.id.as_str());
                key_button = iced::widget::button(
                    iced::widget::text(label.to_owned())
                        .size(style.board.key_label_size)
                        .center(),
                )
                .width(width)
                .height(height)
                .padding(0)
                .on_press_maybe(on_select(key))
                .style(move |theme, status| {
                    let mut appearance = iced::widget::button::secondary(theme, status);
                    appearance.background =
                        Some(iced::Background::Color(iced::Color::from_rgb8(r, g, b)));
                    appearance.text_color =
                        if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 > 128_000 {
                            iced::Color::BLACK
                        } else {
                            iced::Color::WHITE
                        };
                    if selected {
                        appearance.border.width = 2.0;
                        appearance.border.color = theme.extended_palette().primary.strong.color;
                    }
                    appearance
                })
                .into();
            }
            let description = assigned.map_or_else(
                || format!("Physical: {}", key.label),
                |label| format!("Physical: {}\nAssigned: {}", key.label, label.full),
            );
            let key_button = tooltip(key_button, text(description), tooltip::Position::Top);
            board.push(pin(key_button).x(key.x * unit).y(key.y * unit))
        });
        scrollable(
            container(
                board
                    .width(Length::Fixed(board_width))
                    .height(Length::Fixed(board_height)),
            )
            .center_x(Fill),
        )
        .horizontal()
        .width(Fill)
        .into()
    }))
    .height(Length::Fixed(
        rise * style.board.min_unit + style.spacing.s as f32,
    ))
    .width(Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{ActionCategory, ActionChoice, Layer};

    fn key(id: &str, x: f32, y: f32, width: f32) -> PhysicalKey {
        PhysicalKey {
            id: id.into(),
            label: id.into(),
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
    fn displayed_actions_follow_the_selected_draft_layer() {
        let descriptor = Descriptor {
            backend_id: "test".into(),
            device_name: "Test".into(),
            keys: vec![key("Q", 0.0, 0.0, 1.0), key("Scroll Lock", 1.0, 0.0, 1.0)],
            layers: vec![Layer {
                id: "main".into(),
                label: "Main".into(),
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
        let main = labels_for_layer(&descriptor, Some(&draft), "main");
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
            labels_for_layer(&descriptor, Some(&draft), "other")["Q"].compact,
            "Off"
        );
        assert!(labels_for_layer(&descriptor, None, "main").is_empty());
    }

    #[test]
    fn long_or_unknown_actions_stay_legible_and_keep_full_description() {
        let descriptor = Descriptor {
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
        let label = &labels_for_layer(&descriptor, Some(&draft), "main")["one"];
        assert_eq!(label.full, "Factory-specific action");
        assert_eq!(label.compact, "Factor…");
    }
}
