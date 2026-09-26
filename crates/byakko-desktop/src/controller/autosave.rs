//! Debounce deadlines only. Drafts and submitted values remain in core editors.
use crate::config::Config;
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Feature {
    Lighting,
    Picture,
    Settings,
}

#[derive(Default)]
pub struct Autosave {
    due: BTreeMap<Feature, Instant>,
}

impl Autosave {
    pub fn edited(&mut self, feature: Feature, now: Instant, config: &Config) {
        let delay = match feature {
            Feature::Picture => config.auto_save_delay,
            Feature::Lighting | Feature::Settings => config.short_edit_delay,
        };
        self.due.insert(feature, now + delay);
    }

    pub fn cancel(&mut self, feature: Feature) {
        self.due.remove(&feature);
    }

    pub fn clear(&mut self) {
        self.due.clear();
    }

    pub fn pending(&self) -> bool {
        !self.due.is_empty()
    }

    pub fn take_due(&mut self, now: Instant, flush: bool) -> Option<Feature> {
        let feature = self
            .due
            .iter()
            .filter(|(_, at)| flush || **at <= now)
            .min_by_key(|(_, at)| **at)
            .map(|(feature, _)| *feature)?;
        self.due.remove(&feature);
        Some(feature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn repeated_edits_extend_only_their_own_deadline_and_close_flushes_all() {
        let mut saves = Autosave::default();
        let config = Config::default();
        let at = Instant::now();
        saves.edited(Feature::Lighting, at, &config);
        saves.edited(Feature::Picture, at, &config);
        saves.edited(Feature::Lighting, at + Duration::from_millis(100), &config);
        assert_eq!(saves.take_due(at + Duration::from_millis(200), false), None);
        assert_eq!(
            saves.take_due(at + Duration::from_millis(300), false),
            Some(Feature::Lighting)
        );
        assert_eq!(saves.take_due(at + Duration::from_millis(300), false), None);
        assert_eq!(
            saves.take_due(at + Duration::from_millis(300), true),
            Some(Feature::Picture)
        );
        assert!(!saves.pending());
    }
}
