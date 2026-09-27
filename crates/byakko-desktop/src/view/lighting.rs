//! Compact onboard controls and the shared lighting mode dropdown.
use crate::{
    form::lighting::{Form, Message, Mode},
    widget::panels::UiStyle,
};
use byakko_core::{
    editor::{Editor, Status, lighting::LightingRules, picture::PictureRules},
    model::lighting::Content,
    projection::lighting,
};
use iced::{
    Element,
    widget::{button, column, pick_list, text},
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModeChoice {
    mode: Mode,
    label: String,
}
impl std::fmt::Display for ModeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
pub fn mode_selector<'a>(
    lighting: Option<&Editor<LightingRules>>,
    picture: Option<&Editor<PictureRules>>,
    selected: Option<Mode>,
    enabled: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let mut choices = Vec::new();
    if picture.is_some() {
        choices.push(ModeChoice {
            mode: Mode::PerKey,
            label: "Per-key colors".into(),
        });
    }
    if let Some(editor) = lighting {
        let picture_effect =
            picture.and_then(|editor| editor.capabilities().lighting_effect.as_ref());
        choices.extend(
            editor
                .capabilities()
                .effects
                .iter()
                .filter(|effect| Some(&effect.id) != picture_effect)
                .map(|effect| ModeChoice {
                    mode: Mode::Onboard(effect.id.clone()),
                    label: effect.label.clone(),
                }),
        );
        choices.extend(
            editor
                .capabilities()
                .host_modes
                .iter()
                .map(|mode| ModeChoice {
                    mode: Mode::Host(mode.id.clone()),
                    label: mode.label.clone(),
                }),
        );
    }
    let selected = choices
        .iter()
        .find(|choice| Some(&choice.mode) == selected.as_ref())
        .cloned();
    if !enabled {
        choices.retain(|choice| Some(choice) == selected.as_ref());
    }
    column![
        text("Lighting mode"),
        pick_list(choices, selected, |choice: ModeChoice| Message::Mode(
            choice.mode
        ))
        .placeholder("Select lighting mode")
        .width(style.fields.regular)
    ]
    .spacing(style.spacing.xs)
    .into()
}
pub fn view<'a>(
    _form: &'a Form,
    editor: &'a Editor<LightingRules>,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    if !matches!(editor.status(), Status::Ready) {
        let mut content = column![
            text("Lighting needs to be read before editing."),
            button("Retry lighting read").on_press_maybe(idle.then_some(Message::Read))
        ]
        .spacing(style.spacing.s);
        if editor.dirty() {
            content = content
                .push(button("Revert edits").on_press_maybe(idle.then_some(Message::Revert)));
        }
        return content.into();
    }
    let editable = editable && editor.status() == &Status::Ready && editor.draft().is_some();
    if let Some(setting) = editor.draft() {
        match lighting::controls(editor.capabilities(), setting) {
            Ok(controls) => crate::widget::lighting::parameters(
                setting,
                controls.settings,
                editable,
                style,
                format!("lighting:{}", setting.effect),
                Message::Edit,
                Message::Picker,
            ),
            Err(reason) => text(reason).into(),
        }
    } else {
        let explanation = match editor.baseline().map(|s| &s.content) {
            Some(Content::Opaque { reason }) => reason.as_str(),
            Some(Content::HostActive { .. }) => {
                "A host mode is active. Select an onboard effect to replace it."
            }
            _ => "Loading lighting…",
        };
        text(explanation).into()
    }
}
