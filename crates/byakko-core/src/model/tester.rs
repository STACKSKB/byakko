// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure observations from local keyboard input.
//! Times measure observed event spacing and held durations, never device latency or poll rate.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_RECENT_CANDIDATES: usize = 32;
const MAX_INTERVAL_SAMPLES: usize = 128;
pub const DEFAULT_CHATTER_THRESHOLD_US: u64 = 20_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputKind {
    Press,
    Release,
    Repeat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Input {
    /// USB HID keyboard-page usage, independent of the printed legend.
    pub usage: u16,
    pub kind: InputKind,
    pub at_us: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyStats {
    pub presses: u64,
    pub releases: u64,
    pub repeats: u64,
    pub chatter_candidates: u64,
    pub last_hold_us: Option<u64>,
}

/// A rapid release then press. Intentional tapping is indistinguishable here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChatterCandidate {
    pub usage: u16,
    pub previous_make_duration_us: u64,
    pub release_to_next_make_us: u64,
    pub at_us: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntervalSample {
    pub usage: u16,
    pub kind: InputKind,
    /// Spacing since the preceding observed press or release edge.
    pub since_previous_us: u64,
}

#[derive(Clone, Copy, Debug)]
struct Held {
    pressed_at_us: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
struct Released {
    at_us: u64,
    make_duration_us: u64,
}

#[derive(Debug)]
pub struct Tester {
    held: BTreeMap<u16, Held>,
    tested: BTreeSet<u16>,
    stats: BTreeMap<u16, KeyStats>,
    peak_held: usize,
    paused: bool,
    chatter_threshold_us: u64,
    last_event_at_us: Option<u64>,
    released: BTreeMap<u16, Released>,
    recent_candidates: VecDeque<ChatterCandidate>,
    candidate_count: u64,
    last_input: Option<Input>,
    interval_samples: VecDeque<IntervalSample>,
}

impl Default for Tester {
    fn default() -> Self {
        Self {
            held: BTreeMap::new(),
            tested: BTreeSet::new(),
            stats: BTreeMap::new(),
            peak_held: 0,
            paused: false,
            chatter_threshold_us: DEFAULT_CHATTER_THRESHOLD_US,
            last_event_at_us: None,
            released: BTreeMap::new(),
            recent_candidates: VecDeque::new(),
            candidate_count: 0,
            last_input: None,
            interval_samples: VecDeque::new(),
        }
    }
}

impl Tester {
    pub fn accept(&mut self, input: Input) {
        if self.paused {
            return;
        }
        self.last_input = Some(input);
        if self.last_event_at_us.is_some_and(|last| input.at_us < last) {
            // Keep held identity so a following repeat cannot become a make.
            self.released.clear();
            for held in self.held.values_mut() {
                held.pressed_at_us = None;
            }
            self.last_event_at_us = None;
        }
        match input.kind {
            InputKind::Press if self.held.contains_key(&input.usage) => {
                self.last_input = Some(Input {
                    kind: InputKind::Repeat,
                    ..input
                });
                self.stats.entry(input.usage).or_default().repeats += 1;
                return;
            }
            InputKind::Press => {
                if let Some(previous) = self.released.remove(&input.usage) {
                    let gap = input.at_us.saturating_sub(previous.at_us);
                    if gap <= self.chatter_threshold_us {
                        self.candidate_count += 1;
                        self.stats
                            .entry(input.usage)
                            .or_default()
                            .chatter_candidates += 1;
                        push_bounded(
                            &mut self.recent_candidates,
                            ChatterCandidate {
                                usage: input.usage,
                                previous_make_duration_us: previous.make_duration_us,
                                release_to_next_make_us: gap,
                                at_us: input.at_us,
                            },
                            MAX_RECENT_CANDIDATES,
                        );
                    }
                }
                self.held.insert(
                    input.usage,
                    Held {
                        pressed_at_us: Some(input.at_us),
                    },
                );
                self.tested.insert(input.usage);
                self.stats.entry(input.usage).or_default().presses += 1;
                self.peak_held = self.peak_held.max(self.held.len());
            }
            InputKind::Release => {
                self.tested.insert(input.usage);
                self.stats.entry(input.usage).or_default().releases += 1;
                if let Some(held) = self.held.remove(&input.usage) {
                    if let Some(pressed_at) = held.pressed_at_us {
                        self.stats.entry(input.usage).or_default().last_hold_us =
                            Some(input.at_us.saturating_sub(pressed_at));
                        self.released.insert(
                            input.usage,
                            Released {
                                at_us: input.at_us,
                                make_duration_us: input.at_us.saturating_sub(pressed_at),
                            },
                        );
                    }
                } else {
                    self.released.remove(&input.usage);
                }
            }
            InputKind::Repeat => {
                // Orphan repeats, including those after resume, carry no make.
                if self.held.contains_key(&input.usage) {
                    self.stats.entry(input.usage).or_default().repeats += 1;
                } else {
                    return;
                }
                return;
            }
        }
        if let Some(previous) = self.last_event_at_us {
            push_bounded(
                &mut self.interval_samples,
                IntervalSample {
                    usage: input.usage,
                    kind: input.kind,
                    since_previous_us: input.at_us - previous,
                },
                MAX_INTERVAL_SAMPLES,
            );
        }
        self.last_event_at_us = Some(input.at_us);
    }

    /// Stop observing. Focus loss should call this; no synthetic releases count.
    pub fn pause(&mut self) {
        self.paused = true;
        self.held.clear();
        self.released.clear();
        self.last_event_at_us = None;
    }
    pub fn resume(&mut self) {
        self.paused = false;
    }
    /// Clear measurements while retaining held identity until a real release.
    pub fn reset(&mut self) {
        self.tested.clear();
        self.stats.clear();
        self.peak_held = self.held.len();
        self.released.clear();
        self.last_event_at_us = None;
        self.recent_candidates.clear();
        self.candidate_count = 0;
        self.last_input = None;
        self.interval_samples.clear();
        for held in self.held.values_mut() {
            held.pressed_at_us = None;
        }
    }
    pub fn set_chatter_threshold_us(&mut self, threshold_us: u64) {
        self.chatter_threshold_us = threshold_us;
    }
    pub fn chatter_threshold_us(&self) -> u64 {
        self.chatter_threshold_us
    }
    pub fn held(&self) -> impl Iterator<Item = u16> + '_ {
        self.held.keys().copied()
    }
    pub fn tested(&self) -> &BTreeSet<u16> {
        &self.tested
    }
    pub fn stats(&self) -> &BTreeMap<u16, KeyStats> {
        &self.stats
    }
    pub fn peak_held(&self) -> usize {
        self.peak_held
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn last_input(&self) -> Option<Input> {
        self.last_input
    }
    pub fn candidate_count(&self) -> u64 {
        self.candidate_count
    }
    pub fn total_presses(&self) -> u64 {
        self.stats.values().map(|stats| stats.presses).sum()
    }
    pub fn total_releases(&self) -> u64 {
        self.stats.values().map(|stats| stats.releases).sum()
    }
    pub fn total_repeats(&self) -> u64 {
        self.stats.values().map(|stats| stats.repeats).sum()
    }
    pub fn recent_candidates(&self) -> &VecDeque<ChatterCandidate> {
        &self.recent_candidates
    }
    pub fn interval_samples(&self) -> &VecDeque<IntervalSample> {
        &self.interval_samples
    }
}

fn push_bounded<T>(items: &mut VecDeque<T>, item: T, limit: usize) {
    if items.len() == limit {
        items.pop_front();
    }
    items.push_back(item);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(usage: u16, kind: InputKind, at_us: u64) -> Input {
        Input { usage, kind, at_us }
    }

    #[test]
    fn repeats_and_reset_preserve_held_identity() {
        let mut test = Tester::default();
        test.accept(event(4, InputKind::Press, 10));
        test.accept(event(4, InputKind::Press, 11));
        assert_eq!(test.last_input(), Some(event(4, InputKind::Repeat, 11)));
        assert_eq!(
            test.stats()[&4],
            KeyStats {
                presses: 1,
                releases: 0,
                repeats: 1,
                ..Default::default()
            }
        );
        test.reset();
        test.accept(event(4, InputKind::Repeat, 12));
        test.accept(event(4, InputKind::Press, 13));
        assert_eq!(test.stats()[&4].presses, 0);
        assert_eq!(test.stats()[&4].repeats, 2);
        test.accept(event(4, InputKind::Release, 14));
        test.accept(event(4, InputKind::Press, 15));
        assert_eq!(test.stats()[&4].presses, 1);
        assert!(test.recent_candidates().is_empty());
    }

    #[test]
    fn rapid_real_edges_are_candidates_with_both_durations() {
        let mut test = Tester::default();
        test.accept(event(4, InputKind::Press, 1_000));
        test.accept(event(4, InputKind::Release, 4_000));
        test.accept(event(4, InputKind::Press, 14_000));
        assert_eq!(
            test.recent_candidates().back(),
            Some(&ChatterCandidate {
                usage: 4,
                previous_make_duration_us: 3_000,
                release_to_next_make_us: 10_000,
                at_us: 14_000,
            })
        );
        test.accept(event(4, InputKind::Release, 15_000));
        test.accept(event(4, InputKind::Press, 40_001));
        assert_eq!(test.recent_candidates().len(), 1);
        assert_eq!(test.stats()[&4].chatter_candidates, 1);
        assert_eq!(test.stats()[&4].last_hold_us, Some(1_000));
        assert_eq!(test.candidate_count(), 1);
    }

    #[test]
    fn pause_unmatched_release_and_clock_regression_break_chains() {
        let mut test = Tester::default();
        test.accept(event(4, InputKind::Release, 1));
        assert_eq!(test.stats()[&4].last_hold_us, None);
        test.accept(event(4, InputKind::Press, 2));
        assert!(test.recent_candidates().is_empty());
        test.accept(event(4, InputKind::Release, 3));
        test.pause();
        test.accept(event(4, InputKind::Press, 4));
        test.resume();
        test.accept(event(4, InputKind::Repeat, 5));
        test.accept(event(4, InputKind::Press, 6));
        assert!(test.recent_candidates().is_empty());
        test.accept(event(4, InputKind::Release, 7));
        test.accept(event(5, InputKind::Press, 1));
        test.accept(event(4, InputKind::Press, 8));
        assert!(test.recent_candidates().is_empty());
    }

    #[test]
    fn edge_spacing_ignores_repeat_notifications() {
        let mut test = Tester::default();
        test.accept(event(4, InputKind::Press, 0));
        test.accept(event(4, InputKind::Repeat, 100));
        assert_eq!(test.last_input(), Some(event(4, InputKind::Repeat, 100)));
        test.accept(event(4, InputKind::Press, 120));
        test.accept(event(4, InputKind::Release, 1_000));
        assert_eq!(test.interval_samples().len(), 1);
        assert_eq!(test.interval_samples()[0].since_previous_us, 1_000);
        assert_eq!(test.interval_samples()[0].kind, InputKind::Release);
    }
    #[test]
    fn candidate_and_interval_history_are_bounded() {
        let mut test = Tester::default();
        for i in 0..200 {
            test.accept(event(4, InputKind::Press, i * 100));
            test.accept(event(4, InputKind::Release, i * 100 + 1));
        }
        assert_eq!(test.recent_candidates().len(), MAX_RECENT_CANDIDATES);
        assert_eq!(test.interval_samples().len(), MAX_INTERVAL_SAMPLES);
        assert_eq!(test.peak_held(), 1);
        assert_eq!(test.stats()[&4].chatter_candidates, test.candidate_count());
        test.pause();
        assert_eq!(
            test.last_input(),
            Some(event(4, InputKind::Release, 19_901))
        );
        test.reset();
        assert_eq!(test.last_input(), None);
        assert_eq!(test.candidate_count(), 0);
    }
}
