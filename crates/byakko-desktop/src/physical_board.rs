//! Device-neutral projection of physical-key coordinates into an Iced board.
use crate::panels::{self, UiStyle};
use byakko_core::PhysicalKey;
use iced::{
    Element, Fill, Length, Size,
    widget::{column, container, responsive, row, scrollable, space},
};

struct BoardRow<'a> {
    y: f32,
    keys: Vec<&'a PhysicalKey>,
}

fn rows<'a>(keys: &[&'a PhysicalKey]) -> Vec<BoardRow<'a>> {
    let mut ordered = keys.to_vec();
    ordered.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut rows: Vec<BoardRow<'_>> = Vec::new();
    for key in ordered {
        if let Some(last) = rows.last_mut().filter(|row| row.y == key.y) {
            last.keys.push(key);
        } else {
            rows.push(BoardRow {
                y: key.y,
                keys: vec![key],
            });
        }
    }
    rows
}

/// Uses the descriptor's physical geometry. Narrow windows scroll horizontally;
/// normal windows scale up only to the style's maximum key size.
pub fn view<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    let span = keys.iter().map(|key| key.x + key.width).fold(0.0, f32::max);
    let layout = rows(&keys);
    responsive(move |size: Size| {
        let unit = (size.width / span.max(1.0)).clamp(style.board.min_unit, style.board.max_unit);
        let mut board = column![];
        let mut bottom = 0.0;
        for physical_row in &layout {
            let vertical_gap = (physical_row.y - bottom).max(0.0) * unit;
            if vertical_gap > 0.0 {
                board = board.push(space().height(Length::Fixed(vertical_gap)));
            }
            let mut line = row![];
            let mut right = 0.0;
            for key in &physical_row.keys {
                let gap = (key.x - right).max(0.0) * unit;
                if gap > 0.0 {
                    line = line.push(space().width(Length::Fixed(gap)));
                }
                let slot_width = key.width * unit;
                let slot_height = key.height * unit;
                let key_width = (slot_width - style.board.key_gap).max(1.0);
                let key_height = (slot_height - style.board.key_gap).max(1.0);
                line = line.push(panels::selectable_button_with_size(
                    style,
                    key.label.clone(),
                    selected.as_deref() == Some(key.id.as_str()),
                    on_select(key),
                    Some((key_width, key_height)),
                ));
                line = line.push(space().width(Length::Fixed(style.board.key_gap)));
                right = key.x + key.width;
            }
            bottom = physical_row
                .keys
                .iter()
                .map(|key| key.y + key.height)
                .fold(bottom, f32::max);
            board = board.push(line);
        }
        scrollable(container(board).width(Length::Fixed(span * unit)))
            .horizontal()
            .width(Fill)
            .into()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn projects_descriptor_rows_without_board_specific_assumptions() {
        let caps = key("caps", 0.0, 1.0, 1.75);
        let a = key("a", 1.75, 1.0, 1.0);
        let esc = key("esc", 0.0, 0.0, 1.0);
        let projected = rows(&[&a, &caps, &esc]);
        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].keys[0].id, "esc");
        assert_eq!(
            projected[1]
                .keys
                .iter()
                .map(|key| key.id.as_str())
                .collect::<Vec<_>>(),
            ["caps", "a"]
        );
        assert_eq!(projected[1].keys[0].width, 1.75);
    }
}
