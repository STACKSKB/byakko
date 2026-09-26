//! Exclusive host lighting activity; the lighting editor owns restoration state.
use crate::{
    contract::{ApplyFailure, HostEvent, HostEventKind, HostStart, HostTicket, Recovery},
    editor::{Editor, Status, lighting::LightingRules, settings::SettingsRules},
    model::lighting::{HostMode, Setting, Snapshot},
    model::settings,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Idle,
    Starting,
    Active,
    Stopping,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopSource {
    Starting,
    Active,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum State {
    #[default]
    Idle,
    Starting {
        ticket: HostTicket,
        mode_id: String,
    },
    Active {
        ticket: HostTicket,
        mode_id: String,
    },
    Stopping {
        ticket: HostTicket,
        mode_id: String,
        source: StopSource,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostOutcome {
    Ignored,
    Started,
    Stopping,
    Finished,
    Failed(ApplyFailure),
}
pub(crate) struct Plan {
    pub mode: HostMode,
    pub setting: Option<Setting>,
    pub expected: Snapshot,
}
pub(crate) fn plan(
    editor: &Editor<LightingRules>,
    settings: Option<&Editor<SettingsRules>>,
    mode_id: &str,
    setting: Option<Setting>,
) -> Result<Plan, String> {
    if editor.status() != &Status::Ready || editor.dirty() || editor.draft().is_none() {
        return Err("Host lighting requires verified, clean, editable lighting".into());
    }
    let mode = editor
        .capabilities()
        .host_modes
        .iter()
        .find(|mode| mode.id == mode_id)
        .ok_or("Host lighting mode is not advertised")?;
    if let Some(id) = &mode.requires_enabled_setting {
        let settings =
            settings.ok_or("Read and enable the required device setting before host lighting")?;
        let field = settings
            .capabilities()
            .fields
            .iter()
            .find(|field| &field.id == id)
            .filter(|field| field.kind == settings::Kind::Toggle)
            .ok_or("Host lighting requires an advertised toggle setting")?;
        if settings.status() != &Status::Ready {
            return Err(format!("Read {} before host lighting", field.label));
        }
        if !matches!(settings.baseline().map(|snapshot| &snapshot.content),
            Some(settings::Content::Editable(values)) if values.get(id) == Some(&settings::Value::Toggle(true)))
        {
            return Err(format!(
                "Enable and save {} before host lighting",
                field.label
            ));
        }
    }
    let setting = match (&mode.parameters, setting) {
        (None, None) => None,
        (None, Some(_)) => return Err("This host mode does not accept parameters".into()),
        (Some(parameters), setting) => {
            let setting = setting.unwrap_or_else(|| parameters.default.clone());
            crate::validation::lighting::validate_parameters(&parameters.schema, &setting)?;
            Some(setting)
        }
    };
    Ok(Plan {
        mode: mode.clone(),
        setting,
        expected: editor.baseline().ok_or("No lighting baseline")?.clone(),
    })
}
impl State {
    pub fn phase(&self) -> Phase {
        match self {
            Self::Idle => Phase::Idle,
            Self::Starting { .. } => Phase::Starting,
            Self::Active { .. } => Phase::Active,
            Self::Stopping { .. } => Phase::Stopping,
        }
    }
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
    pub fn active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }
    pub fn ticket(&self) -> Option<HostTicket> {
        match self {
            Self::Idle => None,
            Self::Starting { ticket, .. }
            | Self::Active { ticket, .. }
            | Self::Stopping { ticket, .. } => Some(*ticket),
        }
    }
    pub fn mode_id(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Starting { mode_id, .. }
            | Self::Active { mode_id, .. }
            | Self::Stopping { mode_id, .. } => Some(mode_id),
        }
    }
    pub(crate) fn begin(&mut self, start: &HostStart) {
        *self = Self::Starting {
            ticket: start.ticket,
            mode_id: start.mode.id.clone(),
        };
    }
    pub(crate) fn stop(&mut self) -> Option<HostTicket> {
        let ticket = self.ticket()?;
        let source = match self {
            Self::Starting { .. } => StopSource::Starting,
            Self::Active { .. } => StopSource::Active,
            Self::Stopping { .. } => return Some(ticket),
            Self::Idle => return None,
        };
        let mode_id = self.mode_id().expect("host activity owns mode").to_owned();
        *self = Self::Stopping {
            ticket,
            mode_id,
            source,
        };
        Some(ticket)
    }
    pub(crate) fn disconnect(&mut self, editor: &mut Editor<LightingRules>) {
        if !self.is_idle() {
            editor.accept_apply(Err(ApplyFailure {
                message: "Connection lost during host lighting; restoration is unverified".into(),
                recovery: Recovery::Unverified,
            }));
            *self = Self::Idle;
        }
    }
    pub(crate) fn accept(
        &mut self,
        event: HostEvent,
        editor: &mut Editor<LightingRules>,
    ) -> HostOutcome {
        if self.ticket() != Some(event.ticket) {
            return HostOutcome::Ignored;
        }
        match event.kind {
            HostEventKind::Started => match self {
                Self::Starting { mode_id, .. } => {
                    *self = Self::Active {
                        ticket: event.ticket,
                        mode_id: mode_id.clone(),
                    };
                    HostOutcome::Started
                }
                Self::Stopping { source, .. } if *source == StopSource::Starting => {
                    *source = StopSource::Active;
                    HostOutcome::Stopping
                }
                _ => HostOutcome::Ignored,
            },
            HostEventKind::Finished { restored, problem } => {
                let starting = matches!(
                    self,
                    Self::Starting { .. }
                        | Self::Stopping {
                            source: StopSource::Starting,
                            ..
                        }
                );
                *self = Self::Idle;
                match restored {
                    Err(mut failure) => {
                        if let Some(problem) = problem {
                            failure.message =
                                format!("{problem}; restoration: {}", failure.message);
                        }
                        if !starting
                            || matches!(failure.recovery, Recovery::Failed | Recovery::Unverified)
                        {
                            editor.accept_apply(Err(failure.clone()));
                        }
                        HostOutcome::Failed(failure)
                    }
                    Ok(snapshot) => match editor.accept_host_restoration(snapshot) {
                        Err(failure) => {
                            editor.accept_apply(Err(failure.clone()));
                            HostOutcome::Failed(failure)
                        }
                        Ok(()) => problem.map_or(HostOutcome::Finished, |message| {
                            HostOutcome::Failed(ApplyFailure {
                                message,
                                recovery: Recovery::Verified,
                            })
                        }),
                    },
                }
            }
        }
    }
}
