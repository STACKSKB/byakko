//! Recording is a local session activity; it excludes all device commands.
use super::{Activity, Session, Status};
use crate::macros::{
    Action,
    recorder::{DelayPolicy, StopOutcome, Transition},
};

impl Session {
    pub fn start_macro_recording(&mut self, policy: DelayPolicy) -> Result<(), String> {
        self.require_idle()?;
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let editor = self.macros.as_ref().ok_or("Macros are unavailable")?;
        let recorder = editor.recorder(policy)?;
        self.activity = Activity::Recording { recorder };
        self.macro_catalog_operation = None;
        Ok(())
    }

    pub fn record_macro_action(
        &mut self,
        action: Action,
        at_ms: u64,
    ) -> Result<Transition, String> {
        let Activity::Recording { recorder } = &mut self.activity else {
            return Err("Recorder is not active".into());
        };
        self.macros
            .as_mut()
            .ok_or("Macros are unavailable")?
            .record(recorder, action, at_ms)
    }

    pub fn stop_macro_recording(&mut self, at_ms: u64) -> Result<StopOutcome, String> {
        let Activity::Recording { recorder } = &self.activity else {
            return Err("Recorder is not active".into());
        };
        let result = self
            .macros
            .as_mut()
            .ok_or("Macros are unavailable")?
            .stop_recording(recorder, at_ms)?;
        self.activity = Activity::Idle;
        Ok(result)
    }

    pub fn recording(&self) -> bool {
        matches!(self.activity, Activity::Recording { .. })
    }
}
