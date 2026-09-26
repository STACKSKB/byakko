//! Host-mode intent is separate from the onboard lighting draft.
use byakko_core::{
    editor::lighting::edit_parameters,
    model::lighting::{Capabilities, Edit, Setting},
};
use byakko_devices::screen_sample::{DisplaySource, ScreenCapture, ScreenSampling};

#[derive(Clone, Debug)]
pub enum Message {
    Mode(String),
    Parameter(Edit),
    Picker,
    Start,
    Stop,
    RefreshDisplays,
    Displays(Result<Vec<DisplaySource>, String>),
    Display(Option<String>),
    Sampling(ScreenSampling),
}

#[derive(Default)]
pub enum Displays {
    #[default]
    Unloaded,
    Loading,
    Ready(Vec<DisplaySource>),
    Failed(String),
}

#[derive(Default)]
pub struct Form {
    pub mode: Option<String>,
    pub setting: Option<Setting>,
    pub screen: ScreenCapture,
    pub displays: Displays,
}
impl Form {
    pub fn update(&mut self, message: Message, caps: &Capabilities) -> Result<(), String> {
        match message {
            Message::Mode(id) => {
                let mode = caps
                    .host_modes
                    .iter()
                    .find(|mode| mode.id == id)
                    .ok_or("Unknown host mode")?;
                self.setting = mode
                    .parameters
                    .as_ref()
                    .map(|parameters| parameters.default.clone());
                self.mode = Some(id);
            }
            Message::Parameter(edit) => {
                let mode = caps
                    .host_modes
                    .iter()
                    .find(|mode| Some(&mode.id) == self.mode.as_ref())
                    .ok_or("Select a host mode")?;
                let schema = &mode
                    .parameters
                    .as_ref()
                    .ok_or("This mode has no parameters")?
                    .schema;
                self.setting = Some(edit_parameters(
                    schema,
                    self.setting.as_ref().ok_or("Select a host mode")?,
                    edit,
                )?);
            }
            Message::Display(id) => {
                if id.is_none()
                    || matches!(&self.displays, Displays::Ready(displays) if displays.iter().any(|display| Some(&display.id) == id.as_ref()))
                {
                    self.screen.display_id = id;
                } else {
                    return Err("Select an available display".into());
                }
            }
            Message::Sampling(sampling) => {
                if matches!(sampling, ScreenSampling::Point { x, y } if x > 1000 || y > 1000) {
                    return Err("Screen coordinates must be within 0–1000".into());
                }
                self.screen.sampling = sampling;
            }
            Message::Displays(result) if matches!(self.displays, Displays::Loading) => {
                self.displays = match result {
                    Ok(displays) => Displays::Ready(displays),
                    Err(reason) => Displays::Failed(reason),
                };
            }
            Message::Picker
            | Message::Displays(_)
            | Message::RefreshDisplays
            | Message::Start
            | Message::Stop => {}
        }
        Ok(())
    }
}
