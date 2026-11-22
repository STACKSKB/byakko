//! Compact native color picker beside the effect's advertised controls.
use super::{
    color_picker, focus_control,
    panels::{self, UiStyle},
};
use byakko_core::{
    model::lighting::{Color, Edit, Setting},
    projection::lighting::{Control, LevelEdit},
};
use iced::{
    Alignment, Element,
    widget::{column, row, slider, text},
};
pub(crate) fn parameters<'a, M: Clone + 'static>(
    setting: &Setting,
    controls: Vec<Control>,
    editable: bool,
    style: &'a UiStyle,
    picker_id: String,
    edit_message: fn(Edit) -> M,
    picker_message: fn(color_picker::Interaction) -> M,
) -> Element<'a, M> {
    let mut settings = column![].spacing(style.spacing.s);
    for control in controls {
        let control: Element<'a, M> = match control {
            Control::Level {
                edit: LevelEdit::Channel(_),
                ..
            } => continue,
            Control::Choices { label, choices } => row![
                text(label).width(style.fields.compact),
                row(choices.into_iter().map(|choice| panels::selectable_button(
                    style,
                    choice.label,
                    choice.selected,
                    editable.then(|| edit_message(choice.edit))
                )))
                .spacing(style.spacing.xs)
                .width(style.fields.regular)
                .wrap()
            ]
            .spacing(style.spacing.s)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
            Control::Level {
                label,
                range,
                value,
                edit,
            } => {
                let mut controls = row![
                    text(label).width(style.fields.compact),
                    text(value.to_string()).width(style.color_hue_width)
                ]
                .spacing(style.spacing.s)
                .align_y(Alignment::Center);
                if editable && range.start() != range.end() {
                    let first = u32::from(*range.start());
                    let last = u32::from(*range.end());
                    controls = controls.push(focus_control::number(
                        slider(range, value, move |value| {
                            edit_message(edit.edit(value).expect("projected lighting range"))
                        })
                        .step(1u16)
                        .width(style.fields.regular),
                        u32::from(value),
                        first,
                        last,
                        1,
                        editable,
                        move |value| {
                            edit_message(edit.edit(value as u16).expect("projected lighting range"))
                        },
                    ));
                }
                controls.wrap().into()
            }
        };
        settings = settings.push(control);
    }
    let Some(Color::Rgb(rgb)) = setting.color else {
        return settings.into();
    };
    let picker = color_picker::view(
        style,
        rgb,
        picker_id,
        picker_message,
        editable.then_some(move |rgb| edit_message(Edit::Color(Color::Rgb(rgb)))),
    );
    row![picker, settings]
        .spacing(style.spacing.m)
        .wrap()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{
        Rectangle, Size,
        advanced::{
            Layout, layout,
            widget::{Operation, Tree},
        },
    };
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Bounds(BTreeMap<String, Rectangle>, Vec<Rectangle>);
    impl Operation for Bounds {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, rect: Rectangle, value: &str) {
            self.0.insert(value.into(), rect);
        }
        fn focusable(
            &mut self,
            _: Option<&iced::widget::Id>,
            rect: Rectangle,
            _: &mut dyn iced::advanced::widget::operation::focusable::Focusable,
        ) {
            self.1.push(rect);
        }
    }

    #[test]
    fn ripple_parameters_fit_the_inspector_without_fragmenting_labels() {
        use byakko_core::{
            editor::lighting::default_parameters, projection::lighting::parameter_controls,
        };
        let session = byakko_devices::nia87::application::session().unwrap();
        let effect = session
            .lighting()
            .unwrap()
            .capabilities()
            .effects
            .iter()
            .find(|e| e.label == "Ripple")
            .unwrap();
        let mut setting = default_parameters(effect);
        setting.color = Some(Color::Rgb([255, 0, 0]));
        let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0));
        for width in [540.0, 280.0, 360.0, 720.0, 880.0] {
            let mut element = parameters(
                &setting,
                parameter_controls(effect, &setting).unwrap(),
                true,
                &UiStyle::DEFAULT,
                "ripple".into(),
                |_| (),
                |_| (),
            );
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(width, 1000.0)),
            );
            let mut bounds = Bounds::default();
            element
                .as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
            for name in ["Fixed", "Rainbow", "Brightness", "Speed", "Color mode"] {
                let r = bounds.0[name];
                assert!(
                    r.height <= 24.0 && r.x + r.width <= width + 0.5,
                    "{width}: {name} {r:?}"
                );
            }
            // Both numeric controls should retain a useful horizontal pointer target.
            for label in ["Brightness", "Speed"] {
                let r = bounds.0[label];
                assert!(
                    bounds.1.iter().any(|b| b.width >= 120.0
                        && b.height <= 30.0
                        && b.center_y() >= r.y
                        && b.center_y() <= r.y + r.height + 32.0),
                    "{width}: {label} {r:?}, controls {:?}",
                    bounds.1
                );
            }
        }
    }
}
