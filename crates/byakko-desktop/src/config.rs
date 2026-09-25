//! Application timing configuration, supplied by the native composition root.
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub auto_save_delay: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_save_delay: Duration::from_millis(350),
        }
    }
}

impl Config {
    /// Native override; the UI never reads environment variables.
    pub fn from_environment() -> Result<Self, String> {
        match std::env::var("BYAKKO_AUTO_SAVE_DELAY_MS") {
            Ok(value) => Self::from_millis(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn from_millis(value: &str) -> Result<Self, String> {
        let milliseconds: u64 = value
            .parse()
            .map_err(|_| "BYAKKO_AUTO_SAVE_DELAY_MS must be an integer from 0 to 10000")?;
        if milliseconds > 10_000 {
            return Err("BYAKKO_AUTO_SAVE_DELAY_MS must be from 0 to 10000".into());
        }
        Ok(Self {
            auto_save_delay: Duration::from_millis(milliseconds),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_delay_is_validated_and_zero_is_supported() {
        assert_eq!(
            Config::from_millis("725").unwrap().auto_save_delay,
            Duration::from_millis(725)
        );
        assert_eq!(
            Config::from_millis("0").unwrap().auto_save_delay,
            Duration::ZERO
        );
        assert!(Config::from_millis("10001").is_err());
        assert!(Config::from_millis("-1").is_err());
    }
}
