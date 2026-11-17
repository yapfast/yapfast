//! When a profile effect plays and which of its layers show: the clock of
//! `ProfileEffect` and the `shown` of `EffectLayerView`
//! (`ui/controls.slint:860`, `:833`).
//!
//! Slint ticked a `Timer` 24 times a second while the effect played and
//! added 41 ms each time. Here the play's time is the frame times' own,
//! moved on only between two frames it played in, and the widget asks for
//! a frame only when a layer shows or hides, or the play ends; the frames
//! between are its pictures' own, asked for by their players.

use std::time::{Duration, Instant};

use super::EffectLayer;

/// Whether `layer` shows `at` milliseconds into the play: from its start
/// for its duration, and, when it loops, again after each delay, or for
/// good without one. `shown` in `EffectLayerView`.
pub(super) fn shown(layer: &EffectLayer, at: u64) -> bool {
    let Some(since) = at.checked_sub(u64::from(layer.start)) else {
        return false;
    };
    let duration = u64::from(layer.duration);
    if !layer.looped {
        return since <= duration;
    }
    let delay = u64::from(layer.loop_delay);
    delay == 0 || since % (duration + delay) <= duration
}

/// The first millisecond after `at` at which `layer` shows or hides; none
/// once it never changes again.
pub(super) fn next_change(layer: &EffectLayer, at: u64) -> Option<u64> {
    let start = u64::from(layer.start);
    let duration = u64::from(layer.duration);
    let Some(since) = at.checked_sub(start) else {
        return Some(start);
    };
    if !layer.looped {
        return (since <= duration).then_some(start + duration + 1);
    }
    let delay = u64::from(layer.loop_delay);
    if delay == 0 {
        return None;
    }
    let cycle = duration + delay;
    let phase = since % cycle;
    Some(if phase <= duration {
        at + (duration - phase) + 1
    } else {
        at + (cycle - phase)
    })
}

/// Where a play is: Slint's `time`, `finished` and `playing`.
#[derive(Debug, Default)]
pub(super) struct Playback {
    /// It was asked to play at the frame before; a change starts the play
    /// over, as `changed play => { root.time = 0; }`.
    asked: bool,
    /// How far into the play it is.
    time: Duration,
    /// The frame time `time` last moved at, while it played.
    last: Option<Instant>,
    playing: bool,
}

impl Playback {
    /// Takes the frame at `now`: starts over when `asked` changed, and moves
    /// on while it's asked, `may` play (the window plays and there's
    /// something to) and isn't over. Says whether it plays.
    ///
    /// An effect whose layers don't loop is over `end` milliseconds in, and
    /// holds still from then on until asked again.
    pub fn frame(&mut self, asked: bool, may: bool, end: u64, loops: bool, now: Instant) -> bool {
        if asked != self.asked {
            self.asked = asked;
            self.time = Duration::ZERO;
            self.last = None;
        }
        let over = |time: Duration| !loops && millis(time) > end;
        if asked && may && !over(self.time) {
            if let Some(last) = self.last {
                self.time += now.saturating_duration_since(last);
            }
            self.last = Some(now);
        }
        self.playing = asked && may && !over(self.time);
        if !self.playing {
            // Behind another window or held: the time it isn't in front
            // isn't the play's.
            self.last = None;
        }
        self.playing
    }

    /// It played at the last frame.
    pub fn playing(&self) -> bool {
        self.playing
    }

    /// How far into the play it is, in milliseconds.
    pub fn at(&self) -> u64 {
        millis(self.time)
    }

    /// When a frame is next due for the play itself, while it plays: the
    /// first layer to show or hide, or the end of an effect that doesn't
    /// loop.
    pub fn next(
        &self,
        layers: &[EffectLayer],
        end: u64,
        loops: bool,
        now: Instant,
    ) -> Option<Instant> {
        if !self.playing {
            return None;
        }
        let at = self.at();
        let ends = (!loops && at <= end).then_some(end + 1);
        let soonest = layers
            .iter()
            .filter_map(|layer| next_change(layer, at))
            .chain(ends)
            .min()?;
        Some(now + Duration::from_millis(soonest).saturating_sub(self.time))
    }
}

fn millis(time: Duration) -> u64 {
    u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fake Discord's "Petals": an intro once, and a loop that waits a
    /// second between rounds.
    fn petals() -> [EffectLayer; 2] {
        [
            EffectLayer {
                duration: 2000,
                ..EffectLayer::default()
            },
            EffectLayer {
                y: 20.0,
                start: 1500,
                duration: 3000,
                looped: true,
                loop_delay: 1000,
                ..EffectLayer::default()
            },
        ]
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn a_layer_shows_from_its_start_for_its_duration() {
        let [intro, _] = petals();
        assert!(shown(&intro, 0));
        assert!(shown(&intro, 2000));
        assert!(!shown(&intro, 2001));
        assert_eq!(next_change(&intro, 0), Some(2001));
        assert_eq!(next_change(&intro, 2001), None);
    }

    #[test]
    fn a_looping_layer_waits_its_delay_between_rounds() {
        let [_, round] = petals();
        assert!(!shown(&round, 1499));
        assert_eq!(next_change(&round, 0), Some(1500));
        assert!(shown(&round, 1500));
        assert!(shown(&round, 4500));
        assert!(!shown(&round, 4501), "the delay");
        assert_eq!(next_change(&round, 4000), Some(4501));
        assert_eq!(next_change(&round, 4600), Some(5500));
        assert!(shown(&round, 5500), "round again");
    }

    #[test]
    fn a_loop_without_a_delay_shows_for_good() {
        let layer = EffectLayer {
            duration: 500,
            looped: true,
            ..EffectLayer::default()
        };
        assert!(shown(&layer, 10_000));
        assert_eq!(next_change(&layer, 10), None);
    }

    #[test]
    fn an_effect_plays_on_hover_once_then_holds_still() {
        let layers = [petals()[0].clone()];
        let start = Instant::now();
        let mut play = Playback::default();
        assert!(!play.frame(false, true, 2000, false, start));
        assert!(play.frame(true, true, 2000, false, start));
        assert_eq!(
            play.next(&layers, 2000, false, start),
            Some(start + ms(2001))
        );
        assert!(play.frame(true, true, 2000, false, start + ms(1200)));
        assert_eq!(play.at(), 1200);
        assert!(
            !play.frame(true, true, 2000, false, start + ms(2100)),
            "over"
        );
        assert_eq!(play.next(&layers, 2000, false, start + ms(2100)), None);
        assert!(
            !play.frame(true, true, 2000, false, start + ms(9000)),
            "held still"
        );
        // The pointer leaves and comes back: it plays from the start again.
        assert!(!play.frame(false, true, 2000, false, start + ms(9100)));
        assert!(play.frame(true, true, 2000, false, start + ms(9200)));
        assert_eq!(play.at(), 0);
    }

    #[test]
    fn a_looping_effect_plays_while_hovered() {
        let layers = petals();
        let start = Instant::now();
        let mut play = Playback::default();
        assert!(play.frame(true, true, 4500, true, start));
        assert_eq!(
            play.next(&layers, 4500, true, start),
            Some(start + ms(1500))
        );
        assert!(play.frame(true, true, 4500, true, start + ms(60_000)));
        assert_eq!(play.at(), 60_000);
    }

    #[test]
    fn behind_another_window_the_play_stands_still() {
        let start = Instant::now();
        let mut play = Playback::default();
        assert!(play.frame(true, true, 2000, false, start));
        assert!(play.frame(true, true, 2000, false, start + ms(500)));
        assert!(!play.frame(true, false, 2000, false, start + ms(600)));
        assert_eq!(play.next(&petals(), 2000, false, start + ms(600)), None);
        // In front again a minute later: it goes on from where it stopped.
        assert!(play.frame(true, true, 2000, false, start + ms(60_600)));
        assert!(play.frame(true, true, 2000, false, start + ms(60_700)));
        assert_eq!(play.at(), 600);
    }

    #[test]
    fn the_next_frame_lands_on_the_millisecond_a_layer_changes() {
        let layers = petals();
        let start = Instant::now();
        let mut play = Playback::default();
        play.frame(true, true, 4500, true, start);
        play.frame(
            true,
            true,
            4500,
            true,
            start + Duration::from_micros(1_499_600),
        );
        let due = play.next(
            &layers,
            4500,
            true,
            start + Duration::from_micros(1_499_600),
        );
        assert_eq!(due, Some(start + ms(1500)));
    }
}
