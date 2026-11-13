//! `stream_worker`'s consts.

use super::*;

/// The argument that starts Yapfast as a stream worker.
pub const ARGUMENT: &str = "--stream-worker";

/// The environment variable that tells a stream worker how many threads its
/// decoder may use: the share of the machine the Yapfast that starts it works
/// out from the machine's cores and how many streams it's decoding at once.
pub(crate) const THREADS: &str = "YAPFAST_STREAM_THREADS";

/// The largest preview of the shared screen the UI shows.
pub const PREVIEW_WIDTH: u32 = 1280;
pub const PREVIEW_HEIGHT: u32 = 720;
// The sizes a stream is sent and shown at, which the media crate's geometry
// owns: the capture scales to them and so does this worker.
pub(crate) use crate::geometry::{
    CONSTRAINED_HEIGHT, CONSTRAINED_WIDTH, PREVIEW_LONG_EDGE, SHARE_HEIGHT, SHARE_WIDTH,
    WATCH_HEIGHT, WATCH_WIDTH,
};
/// The picture a watched stream starts at, until the UI says how large the
/// tile showing it is: a tile of the call's five-up grid, so a stream shown
/// in one costs what it shows from its first frame. The UI's next tick lifts
/// it to its tile's size, and the frames file grows once to hold that.
pub(crate) const WATCH_TILE_WIDTH: u32 = 480;
pub(crate) const WATCH_TILE_HEIGHT: u32 = 270;
/// The largest preview Discord shows of a stream, and how often its client
/// sends a new one.
pub(crate) const THUMBNAIL_WIDTH: u32 = 512;
pub(crate) const THUMBNAIL_HEIGHT: u32 = 288;
pub(crate) const THUMBNAIL_EVERY: Duration = Duration::from_secs(5 * 60);
/// How long a still that couldn't be read back waits before it is asked for
/// again.
#[cfg(target_os = "linux")]
pub(crate) const STILL_RETRY: Duration = Duration::from_secs(5);

/// How often a share that has produced no picture says what its capture is
/// waiting for: a stream nothing negotiates with is otherwise a long silence.
#[cfg(target_os = "linux")]
pub(crate) const WAITING_EVERY: Duration = Duration::from_secs(5);
/// How many pictures in a row the card's own path may refuse before the share
/// gives it up for the processor's capturer: half a second of a 60 fps screen,
/// which rides out the one buffer a compositor draws in another layout while
/// catching a layout that has changed for good.
#[cfg(nvcuda)]
pub(crate) const REFUSED_PICTURES: u32 = 30;
/// How long a desktop that stopped drawing goes unsaid before the share says so:
/// a compositor sends a picture when it has one, so a share of a screen nobody is
/// drawing to sends nothing, which is worth a line rather than being taken for a
/// stream that stalled.
#[cfg(target_os = "linux")]
pub(crate) const IDLE_EVERY: Duration = Duration::from_secs(5);
/// How long a shared stream whose connection ended waits for a server
/// Discord moved it to.
pub(crate) const MOVE_GRACE: Duration = Duration::from_secs(5);
/// How often a shared screen reports how it goes out, for the call's details.
pub(crate) const SENDING_EVERY: Duration = Duration::from_secs(1);
/// How long a stopping worker has to end by itself before it's ended. A
/// capture stuck in a system call is what takes this long; the stream's
/// threads and memory go with the process.
pub(crate) const STOP_WAIT: Duration = Duration::from_secs(2);
/// The most a preview is taken of a screen whose frames are on the card. A
/// preview is the only thing a share still needs in the processor's memory,
/// and it's a tile a person glances at: ten a second is smoother than the eye
/// reads a thumbnail, and the picture it replaces is a tenth of a second old
/// rather than a frame's.
#[cfg(any(wgc, dup))]
pub(crate) const PREVIEW_EVERY: Duration = Duration::from_millis(100);
/// How many times a share puts the card's session and its capture back
/// together before it stops trying. A driver reset is worth rebuilding both
/// for; a card that answers every frame with the same failure is one to stop
/// sharing on, not to keep asking.
#[cfg(any(wgc, dup))]
pub(crate) const CARD_RESTARTS: u32 = 2;

/// How soon after one picture another is worth converting and scaling: faster
/// than any screen a stream plays on draws, so a picture decoded within this
/// of the one before it is one the next replaces before it's seen.
pub(crate) const SHOW_EVERY: Duration = Duration::from_millis(8);

/// How long [`glance`] waits for a picture of a screen that isn't black,
/// before showing the last it took, or none.
pub(crate) const GLANCE_WAIT: Duration = Duration::from_millis(1500);

/// How many reports may wait for the UI: a share sends a status a second, so
/// 16 seconds of a window that stopped reading still fit, and one that stays
/// stopped never grows the queue past this.
pub(crate) const REPORTS_WAITING: usize = 16;
