use super::*;

/// How long coalesced events wait for company.
///
/// A burst of typing is one call rather than forty; a quarter of a second is
/// short enough that a plugin reacting to someone typing still feels immediate.
pub(super) const COALESCE_MS: u64 = 250;

/// The most events one batch carries, and the most bytes: a busy server can
/// produce more typing than a guest wants to read at once, and a frame is at
/// most 1 MiB.
pub(super) const BATCH_MAX_EVENTS: usize = 64;

pub(super) const BATCH_MAX_BYTES: usize = 256 << 10;

/// How long the helper has to answer the hello, and each kind of request after
/// that. The hook budgets come from the contract; the rest are wide enough for a
/// module to compile on a cold cache and narrow enough that a wedged helper is
/// noticed in seconds.
pub(super) const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// How long an activation gets. It carries the module, so this covers compiling
/// it into wasmtime's cache the first time — work on a cold cache and a busy
/// machine — and then `yf_activate`, which the helper also cancels itself if it
/// runs long. This is the budget for the whole phase.
pub(super) const ACTIVATE_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) const DEACTIVATE_TIMEOUT: Duration = Duration::from_millis(100);

pub(super) const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) const EVICT_TIMEOUT: Duration = Duration::from_millis(500);

/// How long one plugin's call into another may take. The helper holds the same
/// budget for the call it runs; this one answers the caller rather than leaving
/// it waiting for an answer that an instance going away can't give.
pub(super) const CALL_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

/// How long the application waits for the helper to exit before giving up on a
/// graceful stop.
pub(super) const CLOSE_WAIT: Duration = Duration::from_secs(3);

/// How long a plugin's send stands behind the person's last action: §9.1 makes
/// sending without one a capability of its own, and this is what "a recent
/// gesture" means in practice.
pub(super) const GESTURE_WINDOW: Duration = Duration::from_secs(30);

/// `messages.send`'s guard rail: at most this many sends in this window.
pub(super) const SEND_LIMIT: usize = 5;

pub(super) const SEND_WINDOW: Duration = Duration::from_secs(10);

/// The largest picture a plugin may bring for `discord.add_emoji`, and for
/// `discord.add_sticker`: Discord's own caps, so what Yapfast carries is what
/// Discord would take.
pub(super) const MAX_ADDED_EMOJI_BYTES: usize = 256 << 10;

pub(super) const MAX_ADDED_STICKER_BYTES: usize = 512 << 10;

/// The largest sound `audio.play` may bring. A frame carries a megabyte, so
/// this is the sound a plugin can hand over at all.
pub(super) const MAX_PLAYED_SOUND_BYTES: usize = 512 << 10;

/// How long the parent waits for one plugin's decoration: the helper's own
/// budget is 16 ms, and the rest is the frame it takes to get there.
pub(super) const DECORATE_TIMEOUT: Duration = Duration::from_millis(250);

/// The most rows one pass asks a plugin to draw under: a screenful, so a
/// misreported visible range can't turn into a burst of calls.
pub(super) const MAX_DECORATE_ROWS: usize = 64;

/// How long the parent waits for one plugin's answer about a person: the
/// helper's own budget is 16 ms, and the rest is the frame it takes to get
/// there.
pub(super) const IDENTITY_TIMEOUT: Duration = Duration::from_millis(250);

/// The most people one pass asks about: a screenful of authors or a member
/// list's first page, so a misreported range can't turn into a burst of calls.
pub(super) const MAX_IDENTITIES: usize = 64;

/// How long the parent waits for one plugin's colours for a block: the helper's
/// own budget is 16 ms, and the rest is the frame it takes to get there.
pub(super) const CODE_BLOCK_TIMEOUT: Duration = Duration::from_millis(250);

/// The most code blocks one pass asks about: a screenful of messages, each with
/// a block or two, so a misreported range can't turn into a burst of calls.
pub(super) const MAX_CODE_BLOCKS: usize = 64;

/// How often a folder plugin's files are looked at. A save is one `stat` of two
/// files a second, which is nothing beside a build that takes longer than that.
pub(super) const DEV_POLL: Duration = Duration::from_secs(1);

/// The most calls a dependency being replaced may hold for its callers.
///
/// A swap is short, and the callers are waiting on it; a bound keeps a guest
/// that asks in a loop from growing the queue while it waits.
pub(super) const HELD_CALLS_MAX: usize = 64;
