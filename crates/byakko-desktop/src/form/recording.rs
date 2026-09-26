//! Unsubmitted timing options; the core recorder receives a typed policy.
use byakko_core::{model::macros::Capabilities, recorder::macros::DelayPolicy};

#[derive(Clone, Debug)]
pub enum Message {
    Start,
    Stop,
    Fixed(bool),
    Delay(String),
}

pub struct Options {
    pub(crate) fixed: bool,
    pub(crate) delay: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fixed: false,
            delay: "50".into(),
        }
    }
}

impl Options {
    pub fn policy(&self, capabilities: &Capabilities) -> Result<DelayPolicy, String> {
        if self.fixed {
            self.delay
                .parse()
                .map(DelayPolicy::Fixed)
                .map_err(|_| "Enter a whole recording delay in milliseconds".into())
        } else {
            Ok(DelayPolicy::Measured {
                terminal_ms: 50.min(*capabilities.delays_ms.end()),
            })
        }
    }
}
