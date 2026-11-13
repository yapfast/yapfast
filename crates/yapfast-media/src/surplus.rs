//! A queue of sound filled on one clock and played on another, which never run
//! quite together: a sender's sound card and the call's ticks, or the ticks and
//! the speakers. A jitter buffer's own arithmetic, which a call and a watched
//! stream both need and neither owns.

/// Watches a queue of audio filled on one clock and played on another,
/// which never run quite together: a sender's sound card and the call's
/// ticks, or the ticks and the speakers. When the filling one runs fast the
/// queue grows a frame at a time, and the delay with it; this says when a
/// frame can go, once the queue has had frames to spare for a whole window.
pub struct Surplus {
    /// The fewest frames that waited as one was taken, in this window.
    fewest: Option<usize>,
    /// Frames taken in this window.
    counted: u8,
    /// The fewest that waited in the last whole window.
    spare: usize,
    /// How many wait as one is taken while frames keep time: a jitter
    /// buffer's cushion, which isn't surplus.
    cushion: usize,
}

/// Frames are counted in windows of this many: 1 s.
const SURPLUS_WINDOW: u8 = 50;
/// When this many frames more than the cushion waited throughout a window,
/// more than the lateness of their arrivals needed, a quiet one goes.
const SURPLUS: usize = 1;
/// With this many more than the cushion, a frame goes quiet or not.
const SURPLUS_MOST: usize = 4;

impl Default for Surplus {
    /// A queue with no cushion of its own but the frame that comes while
    /// the last plays.
    fn default() -> Self {
        Self::with_cushion(1)
    }
}

/// A decoded frame quieter than this, in mean square of full scale, can be
/// skipped unheard: -40 dBFS.
pub const QUIET: f32 = 1e-4;

impl Surplus {
    /// A queue that keeps `cushion` frames waiting as one is taken.
    pub fn with_cushion(cushion: usize) -> Self {
        Self {
            fewest: None,
            counted: 0,
            spare: 0,
            cushion,
        }
    }

    /// Notes how many frames wait as one is taken to play.
    pub fn note(&mut self, waiting: usize) {
        self.fewest = Some(self.fewest.map_or(waiting, |fewest| fewest.min(waiting)));
        self.counted += 1;
        if self.counted == SURPLUS_WINDOW {
            self.spare = self.fewest.take().unwrap_or(0);
            self.counted = 0;
        }
    }

    /// Whether to skip the frame just taken, for the next: at most one a
    /// window, and only a `quiet` one unless the queue is long.
    pub fn skips(&mut self, quiet: impl FnOnce() -> bool) -> bool {
        let surplus = self.spare.saturating_sub(self.cushion);
        let skips = surplus >= SURPLUS_MOST || (surplus >= SURPLUS && quiet());
        if skips {
            self.spare = 0;
        }
        skips
    }
}
