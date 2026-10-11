// SPDX-License-Identifier: GPL-3.0-or-later
//! Local appearance draft, validation, preview and save acceptance.
use byakko_devices::storage::appearance::{Colors, Mode, Palette, Preferences};
use iced::{Color, Theme, theme::palette};

pub const ROLES: [&str; 7] = [
    "Canvas",
    "Surface",
    "Text",
    "Muted text",
    "Accent",
    "Rule",
    "Focus",
];

#[derive(Clone, Debug)]
pub enum Message {
    Open,
    Cancel,
    Mode(Mode),
    Palette(Palette),
    Colour(usize, String),
    Reset,
    Apply,
}

#[derive(Clone, Debug)]
pub struct Form {
    pub saved: Preferences,
    pub draft: Preferences,
    pub open: bool,
    pub fields: [String; 7],
    pub problem: Option<String>,
    pub submitted: Option<Preferences>,
    load_problem: Option<String>,
}

impl Form {
    pub fn new(loaded: Result<Option<Preferences>, String>) -> Self {
        let (saved, problem) = match loaded {
            Ok(Some(preferences)) => match validate(&preferences) {
                Ok(()) => (preferences, None),
                Err(error) => (
                    Preferences::default(),
                    Some(format!("Saved appearance was invalid: {error}")),
                ),
            },
            Ok(None) => (Preferences::default(), None),
            Err(error) => (
                Preferences::default(),
                Some(format!("Could not load saved appearance: {error}")),
            ),
        };
        Self {
            fields: fields_for(colours(&saved)),
            draft: saved.clone(),
            saved,
            open: false,
            problem: problem.clone(),
            submitted: None,
            load_problem: problem,
        }
    }

    /// A valid edit previews immediately; invalid partial text remains editable.
    pub fn update(&mut self, message: Message) -> Option<Preferences> {
        if self.submitted.is_some() {
            return None;
        }
        match message {
            Message::Open => {
                self.open = true;
                self.draft = self.saved.clone();
                self.fields = fields_for(colours(&self.draft));
                self.problem = self.load_problem.clone();
                None
            }
            Message::Cancel => {
                self.open = false;
                self.draft = self.saved.clone();
                self.fields = fields_for(colours(&self.draft));
                self.problem = self.load_problem.clone();
                Some(self.saved.clone())
            }
            Message::Mode(mode) if self.open => {
                self.draft.mode = mode;
                self.fields = fields_for(colours(&self.draft));
                self.preview()
            }
            Message::Palette(palette) if self.open => {
                self.draft.palette = palette;
                self.fields = fields_for(colours(&self.draft));
                self.preview()
            }
            Message::Colour(index, value) if self.open && index < self.fields.len() => {
                if self.draft.palette != Palette::Custom {
                    let selected = editable_colours(&self.draft);
                    self.draft.custom[side(self.draft.mode)] = selected;
                    self.draft.palette = Palette::Custom;
                    self.fields = fields_for(selected);
                }
                self.fields[index] = value;
                match parse_fields(&self.fields) {
                    Ok(colors) => {
                        self.draft.custom[side(self.draft.mode)] = colors;
                        self.preview()
                    }
                    Err(error) => {
                        self.problem = Some(error);
                        None
                    }
                }
            }
            Message::Reset if self.open => {
                self.draft.custom[side(self.draft.mode)] =
                    Preferences::default().custom[side(self.draft.mode)];
                self.draft.palette = Palette::Custom;
                self.fields = fields_for(colours(&self.draft));
                self.preview()
            }
            Message::Apply
            | Message::Mode(_)
            | Message::Palette(_)
            | Message::Colour(_, _)
            | Message::Reset => None,
        }
    }

    fn preview(&mut self) -> Option<Preferences> {
        match validate(&self.draft) {
            Ok(()) => {
                self.problem = None;
                Some(self.draft.clone())
            }
            Err(error) => {
                self.problem = Some(error);
                None
            }
        }
    }

    /// Freeze the exact validated target until the file write settles.
    pub fn begin_apply(&mut self) -> Result<Preferences, String> {
        if !self.open || self.submitted.is_some() {
            return Err("Appearance is not ready to save.".into());
        }
        if self.draft.palette == Palette::Custom {
            self.draft.custom[side(self.draft.mode)] =
                parse_fields(&self.fields).inspect_err(|error| {
                    self.problem = Some(error.clone());
                })?;
        }
        validate(&self.draft).inspect_err(|error| {
            self.problem = Some(error.clone());
        })?;
        let target = self.draft.clone();
        self.submitted = Some(target.clone());
        self.problem = None;
        Ok(target)
    }

    /// Failed persistence keeps every draft field for correction or retry.
    pub fn accept(&mut self, result: Result<(), String>) {
        let Some(submitted) = self.submitted.take() else {
            return;
        };
        match result {
            Ok(()) => {
                self.saved = submitted.clone();
                self.draft = submitted;
                self.open = false;
                self.problem = None;
                self.load_problem = None;
            }
            Err(error) => self.problem = Some(error),
        }
    }
}

impl Default for Form {
    fn default() -> Self {
        Self::new(Ok(None))
    }
}

fn side(mode: Mode) -> usize {
    match mode {
        Mode::Light => 0,
        Mode::Dark => 1,
    }
}

/// Resolved seven-role colors of the selected preset and light/dark mode.
pub fn colours(preferences: &Preferences) -> Colors {
    let dark = preferences.mode == Mode::Dark;
    match preferences.palette {
        Palette::Default => {
            let theme = if dark { Theme::Dark } else { Theme::Light };
            let palette = theme.extended_palette();
            let rgb = |color: Color| {
                let [red, green, blue, _] = color.into_rgba8();
                [red, green, blue]
            };
            Colors {
                canvas: rgb(palette.background.base.color),
                surface: rgb(palette.background.weak.color),
                text: rgb(palette.background.base.text),
                muted: rgb(palette.background.base.text),
                accent: rgb(palette.primary.base.color),
                rule: rgb(palette.background.strong.color),
                focus: rgb(palette.primary.strong.color),
            }
        }
        Palette::Custom => preferences.custom[side(preferences.mode)],
        Palette::Slate if !dark => Colors {
            canvas: [0xf0, 0xf2, 0xf3],
            surface: [0xff, 0xff, 0xff],
            text: [0x25, 0x2b, 0x31],
            muted: [0x62, 0x6d, 0x77],
            accent: [0x48, 0x65, 0x7b],
            rule: [0xd3, 0xd9, 0xde],
            focus: [0x48, 0x65, 0x7b],
        },
        Palette::Slate => Colors {
            canvas: [0x1d, 0x22, 0x28],
            surface: [0x27, 0x2e, 0x35],
            text: [0xe1, 0xe7, 0xec],
            muted: [0xa9, 0xb5, 0xc0],
            accent: [0xa7, 0xcc, 0xe7],
            rule: [0x3e, 0x49, 0x53],
            focus: [0xa7, 0xcc, 0xe7],
        },
        Palette::Forest if !dark => Colors {
            canvas: [0xf1, 0xf3, 0xed],
            surface: [0xff, 0xff, 0xff],
            text: [0x28, 0x30, 0x24],
            muted: [0x65, 0x70, 0x5c],
            accent: [0x4d, 0x6b, 0x48],
            rule: [0xd2, 0xd9, 0xca],
            focus: [0x4d, 0x6b, 0x48],
        },
        Palette::Forest => Colors {
            canvas: [0x1d, 0x24, 0x1e],
            surface: [0x27, 0x2f, 0x27],
            text: [0xe3, 0xe9, 0xdf],
            muted: [0xb0, 0xbc, 0xa8],
            accent: [0xb3, 0xd0, 0x9e],
            rule: [0x41, 0x4f, 0x3e],
            focus: [0xb3, 0xd0, 0x9e],
        },
        Palette::StacksKB if !dark => Colors {
            canvas: [0xf7, 0xf7, 0xf7],
            surface: [0xff, 0xff, 0xff],
            text: [0x00, 0x00, 0x00],
            muted: [0x50, 0x50, 0x50],
            accent: [0x00, 0x00, 0x00],
            rule: [0xed, 0xed, 0xed],
            focus: [0x2b, 0x6c, 0xb0],
        },
        Palette::StacksKB => Colors {
            canvas: [0x18, 0x18, 0x18],
            surface: [0x24, 0x24, 0x24],
            text: [0xf7, 0xf7, 0xf7],
            muted: [0xb8, 0xb8, 0xb8],
            accent: [0xed, 0xed, 0xed],
            rule: [0x50, 0x50, 0x50],
            focus: [0x6f, 0xa7, 0xdf],
        },
    }
}

fn color(rgb: [u8; 3]) -> Color {
    Color::from_rgb8(rgb[0], rgb[1], rgb[2])
}

// Iced's stock focus colour can miss the stricter contrast of saved custom
// palettes. Copy the native value when possible, otherwise use its text colour.
fn editable_colours(preferences: &Preferences) -> Colors {
    let mut selected = colours(preferences);
    if [selected.canvas, selected.surface]
        .into_iter()
        .any(|background| contrast(selected.focus, background) < 3.0)
    {
        selected.focus = selected.text;
    }
    selected
}

/// The default preset keeps Iced's existing light/dark appearance unchanged.
pub fn theme(preferences: &Preferences) -> Theme {
    if preferences.palette == Palette::Default {
        return match preferences.mode {
            Mode::Light => Theme::Light,
            Mode::Dark => Theme::Dark,
        };
    }
    let colors = colours(preferences);
    let base = match preferences.mode {
        Mode::Light => Theme::Light.palette(),
        Mode::Dark => Theme::Dark.palette(),
    };
    let readable_status = |candidate: Color| {
        let [red, green, blue, _] = candidate.into_rgba8();
        if [colors.canvas, colors.surface]
            .iter()
            .all(|surface| contrast([red, green, blue], *surface) >= 4.5)
        {
            candidate
        } else {
            color(colors.text)
        }
    };
    let palette = palette::Palette {
        background: color(colors.canvas),
        text: color(colors.text),
        primary: color(colors.accent),
        success: readable_status(base.success),
        warning: readable_status(base.warning),
        danger: readable_status(base.danger),
    };
    Theme::custom_with_fn("Byakko appearance", palette, move |palette| {
        let mut extended = palette::Extended::generate(palette);
        let canvas = color(colors.canvas);
        let surface = color(colors.surface);
        let text = color(colors.text);
        let muted = color(colors.muted);
        let rule = color(colors.rule);
        let focus = color(colors.focus);
        extended.background.base = palette::Pair::new(canvas, text);
        extended.background.weakest = palette::Pair::new(surface, text);
        extended.background.weaker = palette::Pair::new(surface, text);
        extended.background.weak = palette::Pair::new(surface, text);
        extended.background.strong = palette::Pair::new(rule, text);
        extended.secondary.base = palette::Pair {
            color: surface,
            text: muted,
        };
        extended.secondary.weak = palette::Pair {
            color: surface,
            text: muted,
        };
        extended.secondary.strong = palette::Pair::new(rule, text);
        extended.primary.strong = palette::Pair::new(focus, text);
        extended
    })
}

fn fields_for(colors: Colors) -> [String; 7] {
    [
        colors.canvas,
        colors.surface,
        colors.text,
        colors.muted,
        colors.accent,
        colors.rule,
        colors.focus,
    ]
    .map(|rgb| format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
}

fn parse_field(index: usize, value: &str) -> Result<[u8; 3], String> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err(format!("{} needs a #RRGGBB colour.", ROLES[index]));
    }
    let byte = |offset| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap();
    Ok([byte(1), byte(3), byte(5)])
}

fn parse_fields(fields: &[String; 7]) -> Result<Colors, String> {
    let mut rgb = [[0; 3]; 7];
    for (index, field) in fields.iter().enumerate() {
        rgb[index] = parse_field(index, field)?;
    }
    Ok(Colors {
        canvas: rgb[0],
        surface: rgb[1],
        text: rgb[2],
        muted: rgb[3],
        accent: rgb[4],
        rule: rgb[5],
        focus: rgb[6],
    })
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    f64::from(crate::widget::controls::contrast(color(a), color(b)))
}

fn validate_colors(colors: Colors) -> Result<(), String> {
    for background in [colors.canvas, colors.surface] {
        if contrast(colors.text, background) < 4.5 || contrast(colors.muted, background) < 4.5 {
            return Err("Text and muted text need 4.5:1 contrast on canvas and surface.".into());
        }
        if contrast(colors.focus, background) < 3.0 {
            return Err("Focus colour needs 3:1 contrast on canvas and surface.".into());
        }
    }
    Ok(())
}

pub fn validate(preferences: &Preferences) -> Result<(), String> {
    for colors in preferences.custom {
        validate_colors(colors)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_surfaces_keep_validation_errors_readable() {
        let mut preferences = Preferences {
            mode: Mode::Light,
            palette: Palette::Custom,
            ..Preferences::default()
        };
        preferences.custom[0].canvas = [0xa4, 0x38, 0x2c];
        preferences.custom[0].surface = [0xa4, 0x38, 0x2c];
        preferences.custom[0].text = [255; 3];
        preferences.custom[0].muted = [255; 3];
        preferences.custom[0].focus = [255; 3];
        assert!(validate(&preferences).is_ok());
        let [red, green, blue, _] = theme(&preferences).palette().danger.into_rgba8();
        assert!(contrast([red, green, blue], preferences.custom[0].canvas) >= 4.5);
    }

    #[test]
    fn presets_match_tokens_and_default_theme_remains_native() {
        let mut preferences = Preferences::default();
        assert_eq!(theme(&preferences), Theme::Dark);
        let dark = Theme::Dark.extended_palette();
        assert_eq!(
            colours(&preferences).canvas,
            dark.background.base.color.into_rgba8()[..3]
        );
        assert_eq!(
            colours(&preferences).focus,
            dark.primary.strong.color.into_rgba8()[..3]
        );
        preferences.mode = Mode::Light;
        assert_eq!(theme(&preferences), Theme::Light);
        preferences.palette = Palette::Slate;
        assert_eq!(colours(&preferences).canvas, [0xf0, 0xf2, 0xf3]);
        preferences.palette = Palette::Forest;
        assert_eq!(colours(&preferences).accent, [0x4d, 0x6b, 0x48]);
        preferences.palette = Palette::StacksKB;
        assert_eq!(colours(&preferences).focus, [0x2b, 0x6c, 0xb0]);
        preferences.mode = Mode::Dark;
        assert_eq!(colours(&preferences).canvas, [0x18, 0x18, 0x18]);
    }

    #[test]
    fn invalid_partial_colour_and_contrast_hold_preview_and_draft_until_fixed() {
        let mut form = Form::default();
        form.update(Message::Open);
        assert!(form.update(Message::Palette(Palette::Slate)).is_some());
        assert_eq!(form.draft.palette, Palette::Slate);
        assert!(form.update(Message::Colour(0, "#1".into())).is_none());
        assert_eq!(form.draft.palette, Palette::Custom);
        assert_eq!(form.fields[0], "#1");
        assert_eq!(form.draft.custom[1].surface, [0x27, 0x2e, 0x35]);
        assert!(form.begin_apply().is_err());
        assert!(form.update(Message::Colour(0, "#FFFFFF".into())).is_none());
        assert!(form.problem.as_deref().unwrap().contains("contrast"));
        assert!(form.update(Message::Colour(0, "#1D2228".into())).is_some());
        assert!(form.problem.is_none());
    }

    #[test]
    fn first_colour_edit_copies_the_visible_native_preset() {
        let mut form = Form::default();
        form.update(Message::Open);
        let visible = colours(&form.draft);
        let original = form.fields[0].clone();
        let preview = form.update(Message::Colour(0, original));
        assert!(preview.is_some(), "{:?}", form.problem);
        assert_eq!(form.draft.palette, Palette::Custom);
        let editable = form.draft.custom[1];
        assert_eq!(editable.canvas, visible.canvas);
        assert_eq!(editable.surface, visible.surface);
        assert_eq!(editable.text, visible.text);
        assert_eq!(editable.muted, visible.text);
        assert_eq!(editable.focus, visible.text);
        assert_eq!(form.fields[6], fields_for(editable)[6]);
    }

    #[test]
    fn named_palettes_are_readable_in_both_modes() {
        let mut preferences = Preferences::default();
        for mode in [Mode::Light, Mode::Dark] {
            preferences.mode = mode;
            for palette in [Palette::Slate, Palette::Forest, Palette::StacksKB] {
                preferences.palette = palette;
                assert!(
                    validate_colors(colours(&preferences)).is_ok(),
                    "{mode:?} {palette:?}"
                );
            }
        }
    }

    #[test]
    fn cancel_and_failed_save_preserve_the_right_values() {
        let mut form = Form::default();
        form.update(Message::Open);
        let preview = form.update(Message::Mode(Mode::Light)).unwrap();
        assert_eq!(preview.mode, Mode::Light);
        assert_eq!(form.update(Message::Cancel), Some(form.saved.clone()));
        assert!(!form.open);
        form.update(Message::Open);
        form.update(Message::Mode(Mode::Light));
        let target = form.begin_apply().unwrap();
        assert_eq!(target.mode, Mode::Light);
        assert!(form.update(Message::Cancel).is_none());
        assert!(form.update(Message::Mode(Mode::Dark)).is_none());
        assert!(form.open);
        assert_eq!(form.draft.mode, Mode::Light);
        form.accept(Err("disk full".into()));
        assert!(form.open);
        assert_eq!(form.draft.mode, Mode::Light);
        assert_eq!(form.saved.mode, Mode::Dark);
        assert!(form.problem.as_deref().unwrap().contains("disk full"));
        form.begin_apply().unwrap();
        form.accept(Ok(()));
        assert_eq!(form.saved.mode, Mode::Light);
        assert!(!form.open);
    }

    #[test]
    fn invalid_saved_colours_fall_back_to_default_with_diagnostic() {
        let mut saved = Preferences::default();
        saved.custom[0].text = saved.custom[0].canvas;
        let form = Form::new(Ok(Some(saved)));
        assert_eq!(form.saved, Preferences::default());
        assert!(form.problem.is_some());
    }

    #[test]
    fn hidden_custom_mode_must_also_be_readable_before_save() {
        let mut form = Form::default();
        form.update(Message::Open);
        form.draft.custom[0].muted = form.draft.custom[0].surface;
        assert!(form.begin_apply().is_err());
        assert!(form.submitted.is_none());
        assert!(form.open);
        assert_eq!(form.saved, Preferences::default());
    }
}
