//! Small, coalesced UI intent queue for onboard lighting edits.
use byakko_core::lighting::{self, Content, Edit, Setting, editor::Editor};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub(crate) struct Pending {
    edits: Vec<Edit>,
    due: Option<Instant>,
    blocked: bool,
}

impl Pending {
    pub(crate) fn postpone(&mut self, now: Instant, delay: Duration) {
        if self
            .edits
            .iter()
            .any(|edit| matches!(edit, Edit::Color(_) | Edit::Channel(_, _)))
        {
            self.due = Some(now + delay);
        }
    }
    pub fn has_pending(&self) -> bool {
        !self.blocked && !self.edits.is_empty()
    }

    pub fn has_queued(&self) -> bool {
        !self.edits.is_empty()
    }

    pub fn blocked(&self) -> bool {
        self.blocked
    }

    pub fn retry(&mut self) {
        self.blocked = false;
        self.due = None;
    }

    pub fn block(&mut self) {
        self.blocked = true;
    }

    pub fn clear(&mut self) {
        self.edits.clear();
        self.due = None;
        self.blocked = false;
    }

    pub fn ready(&self, now: Instant) -> bool {
        self.has_pending() && self.due.is_none_or(|due| now >= due)
    }

    pub fn push(&mut self, edit: Edit, now: Instant, delay: Duration) {
        self.blocked = false;
        if matches!(edit, Edit::Effect(_)) {
            self.edits.clear();
        } else {
            self.edits.retain(|earlier| !same_field(earlier, &edit));
            if matches!(edit, Edit::Color(_)) {
                self.edits
                    .retain(|earlier| !matches!(earlier, Edit::Channel(_, _)));
            }
        }
        let coloring = matches!(edit, Edit::Color(_) | Edit::Channel(_, _));
        self.edits.push(edit);
        debug_assert!(self.edits.len() <= 8);
        self.due = coloring.then_some(now + delay);
    }

    pub fn projected(&self, editor: &Editor) -> Result<Option<Setting>, String> {
        let mut setting = editor.draft().cloned();
        for edit in &self.edits {
            setting = Some(match (setting, edit) {
                (Some(current), edit) => {
                    lighting::edit(editor.capabilities(), &current, edit.clone())?
                }
                (None, Edit::Effect(id))
                    if matches!(
                        editor.baseline().map(|snapshot| &snapshot.content),
                        Some(Content::HostActive { .. })
                    ) =>
                {
                    lighting::default_setting(editor.capabilities(), id)?
                }
                _ => return Err("Lighting is not editable".into()),
            });
        }
        Ok(setting)
    }
}

fn same_field(left: &Edit, right: &Edit) -> bool {
    match (left, right) {
        (Edit::Effect(_), Edit::Effect(_))
        | (Edit::Brightness(_), Edit::Brightness(_))
        | (Edit::Speed(_), Edit::Speed(_))
        | (Edit::Option(_), Edit::Option(_))
        | (Edit::Color(_), Edit::Color(_)) => true,
        (Edit::Channel(a, _), Edit::Channel(b, _)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::lighting::{Channel, Color};

    #[test]
    fn slider_intent_coalesces_and_effect_resets_old_parameters() {
        let now = Instant::now();
        let delay = Duration::from_millis(350);
        let mut pending = Pending::default();
        pending.push(Edit::Brightness(10), now, delay);
        pending.push(Edit::Brightness(20), now, delay);
        assert_eq!(pending.edits, vec![Edit::Brightness(20)]);
        assert!(pending.ready(now));
        pending.push(Edit::Color(Color::Rgb([1, 2, 3])), now, delay);
        assert!(!pending.ready(now));
        assert!(pending.ready(now + delay));
        pending.push(Edit::Channel(Channel::Red, 4), now, delay);
        pending.push(Edit::Color(Color::Rainbow), now, delay);
        assert_eq!(pending.edits.len(), 2);
        pending.push(Edit::Effect("other".into()), now, delay);
        assert_eq!(pending.edits, vec![Edit::Effect("other".into())]);
        assert!(pending.ready(now));
        pending.postpone(now, delay);
        assert!(pending.ready(now));
        pending.block();
        assert!(!pending.has_pending());
        assert!(pending.has_queued());
        pending.retry();
        assert!(pending.ready(now));
    }
}
