//! A moving picture's frames, decoded off the window's thread as they show,
//! made at the size and in the shape they're drawn. Ported from `Frames`,
//! `Cursor` and `decode_frames` in `src/ui/pictures.rs`, and the queue and
//! cache of `src/ui/emoji_animation.rs`.
//!
//! A run over the frames is a thread that decodes them in order and round
//! again, one ahead of the one showing, passing over those before the frame
//! asked for. Each place showing the animation at its own frame has a run
//! of its own, so two don't send each other back to the start. A run whose
//! next frame nobody takes for a while lets go of it and ends by itself, as
//! when its picture scrolled away or the window went behind another: no
//! timer stays to sweep it, and a window at rest wakes no thread.
//!
//! An animation whose frames together take a small share of the store's
//! budget, as an emoji's or a small sticker's, keeps each frame it's given;
//! once it has them all its runs end, and it plays them again with no
//! thread decoding, as Slint's emoji cache did. Only large stickers and
//! GIFs stream for as long as they play.
//!
//! A source that gives fewer frames than it has delays, as a file that
//! won't open, a Lottie that won't parse or a GIF corrupt part way, plays
//! the frames it gave and holds the last, or the still when none came: the
//! run that found it short says how many there are, and no run starts for
//! those past them, as Slint's cursor stayed where its thread ended. Its
//! first frame is the still's, so one kept whole needs no run again.

mod run;

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::image;

use super::look::Look;
use super::moving::Moving;
use run::{Run, not_past};

/// How long a run waits for its next frame to be taken before it ends, at
/// the least; a slow animation's run waits two of its longest frames.
const IDLE: Duration = Duration::from_secs(1);
/// How many places one animation plays from at once, each at its own
/// frame, as the same GIF posted twice does.
const RUNS: usize = 4;
/// How many frames a run may be behind the frame asked for and still give
/// it, passing over those between; further back, the asker is another
/// place showing the animation at its own frame.
const CATCH_UP: usize = 4;
/// The most an animation's frames may take together to be kept whole: a
/// sixteenth of the store's budget, as 128 frames of 32 by 32 pixels, an
/// emoji at 1x, or 32 frames of 64 by 64.
const KEEP: usize = super::store::BUDGET / 16;

/// One animation as one look draws it: its runs, and the frames given. It
/// holds the picture only through its runs, so one that ended keeps nothing
/// of the core's.
pub(crate) struct Animation {
    look: Look,
    count: usize,
    /// How many frames the source gives: `count`, until a run found fewer.
    gives: usize,
    idle: Duration,
    runs: Vec<Run>,
    /// The frame a run last gave and its index, which shows again while
    /// the frame asked for is on its way, and as long as that frame is asked
    /// for, as by a picture held still, without a run.
    last: Option<(usize, image::Handle)>,
    /// Each frame given so far, for one small enough to keep; none for one
    /// that streams.
    kept: Vec<Option<image::Handle>>,
    /// How many of `kept` are in: all, and no run is needed again.
    filled: usize,
}

impl Animation {
    pub fn new(source: &dyn Moving, look: Look) -> Self {
        let count = source.delays().len();
        let longest = source.delays().iter().copied().max().unwrap_or(0);
        let idle = IDLE.max(Duration::from_millis(u64::from(longest) * 2));
        let small = count.saturating_mul(look.bytes()) <= KEEP;
        Self {
            look,
            count,
            gives: count,
            idle,
            runs: Vec::new(),
            last: None,
            kept: if small { vec![None; count] } else { Vec::new() },
            filled: 0,
        }
    }

    /// Whether frame `index` is kept.
    fn has(&self, index: usize) -> bool {
        self.kept.get(index).is_some_and(Option::is_some)
    }

    /// Whether every frame the source gives is kept, and no run is needed
    /// again: of one that ran short, every one past the first, which the
    /// still shows.
    fn whole(&self) -> bool {
        !self.kept.is_empty()
            && (self.filled == self.count
                || self.gives < self.count && (1..self.gives).all(|row| self.has(row)))
    }

    /// Keeps the frame a run gave, if it keeps any; once it has every one,
    /// lets go of its runs, which ends their threads.
    fn keep(&mut self, index: usize, frame: &image::Handle) {
        let Some(slot) = self.kept.get_mut(index) else {
            return;
        };
        if slot.is_none() {
            *slot = Some(frame.clone());
            self.filled += 1;
            if self.whole() {
                self.runs.clear();
            }
        }
    }

    /// Lets go of the runs that ended, noting how many frames the source
    /// gives if one found it short.
    fn sweep(&mut self) {
        let mut gives = self.gives;
        self.runs.retain(|run| {
            let state = run.hand.lock();
            if let Some(given) = state.gives {
                gives = gives.min(given);
            }
            !state.ended
        });
        self.gives = gives;
    }

    /// Frame `row`: from the run showing it or the nearest one coming up to
    /// it, otherwise from a run of its own started there, in place of the
    /// one used longest ago if there are enough. Until a frame is decoded
    /// the one before shows; none before the first, when the still shows.
    /// The still is the first frame, so asked for that before any other, a
    /// run makes the second ready instead. Once every frame is kept, each
    /// is given at once. Past the frames the source gives, the last given
    /// holds, and no run starts.
    pub fn frame(
        &mut self,
        row: usize,
        now: Instant,
        source: &Arc<dyn Moving>,
    ) -> Option<image::Handle> {
        if row >= self.count {
            return None;
        }
        if let Some((index, frame)) = &self.last
            && *index == row
        {
            return Some(frame.clone());
        }
        self.sweep();
        if row >= self.gives {
            return self.last.as_ref().map(|(_, frame)| frame.clone());
        }
        if self.whole() {
            self.runs.clear();
            // None for the first of one that ran short: the still is it.
            let frame = self.kept[row].clone()?;
            self.last = Some((row, frame.clone()));
            return Some(frame);
        }
        // The still the first time round, and every time round for a source
        // that ran short, whose run ended: its next run starts a frame early.
        if row == 0 && (self.last.is_none() || self.gives < self.count) {
            if self.runs.is_empty() && 1 < self.gives {
                self.run_for(1, now, source);
            }
            return None;
        }
        let at = self.run_for(row, now, source);
        let run = &mut self.runs[at];
        run.advance(row, self.count);
        let Some((index, frame)) = run.shown.clone() else {
            return self.last.as_ref().map(|(_, frame)| frame.clone());
        };
        self.keep(index, &frame);
        self.last = Some((index, frame.clone()));
        Some(frame)
    }

    /// The run that gives `row`, started if none is near it, and used now;
    /// those that ended are gone (`sweep`).
    fn run_for(&mut self, row: usize, now: Instant, source: &Arc<dyn Moving>) -> usize {
        let count = self.count;
        let at = self
            .runs
            .iter()
            .position(|run| run.shown.as_ref().is_some_and(|(shown, _)| *shown == row))
            .or_else(|| {
                self.runs
                    .iter()
                    .enumerate()
                    .filter(|(_, run)| not_past(run.next, row, count))
                    .map(|(at, run)| (at, (row + count - run.next) % count))
                    .filter(|(_, behind)| *behind <= CATCH_UP)
                    .min_by_key(|(_, behind)| *behind)
                    .map(|(at, _)| at)
            });
        let at = at.unwrap_or_else(|| {
            let fresh = Run::start(source, self.look, row, self.idle, now);
            if self.runs.len() < RUNS {
                self.runs.push(fresh);
                self.runs.len() - 1
            } else {
                let at = self
                    .runs
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, run)| run.used)
                    .map_or(0, |(at, _)| at);
                self.runs[at] = fresh;
                at
            }
        });
        self.runs[at].used = now;
        at
    }

    /// What it holds: the frames kept, the frame last given if it isn't one
    /// of them, and one on its way in each run.
    pub fn bytes(&self) -> usize {
        let live = self.runs.iter().filter(|run| !run.ended()).count();
        let last = self
            .last
            .as_ref()
            .is_some_and(|(index, _)| !self.has(*index));
        (self.filled + usize::from(last) + live) * self.look.bytes()
    }

    /// How many runs are going, for the tests.
    #[cfg(test)]
    pub fn runs(&self) -> usize {
        self.runs.iter().filter(|run| !run.ended()).count()
    }
}

#[cfg(test)]
mod tests;
