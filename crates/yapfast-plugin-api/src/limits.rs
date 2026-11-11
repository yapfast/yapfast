//! The caps the plugin system holds itself to.
//!
//! They live here so the parent and the helper refuse the same thing at the
//! same size, and so the docs can name one number.

/// A `.wasm` file larger than this is refused before it is compiled.
pub const MAX_WASM_BYTES: u64 = 8 << 20;

/// The most one event or hook payload carries. Fields over it are truncated
/// and flagged rather than refused, so a plugin still sees the event.
pub const MAX_PAYLOAD_BYTES: usize = 64 << 10;

/// Pages of guest memory an instance gets when its manifest doesn't say: 16 MiB.
pub const DEFAULT_MEMORY_PAGES: u32 = 256;
/// Fewer pages than this can't hold a Rust PDK plugin and its input.
pub const MIN_MEMORY_PAGES: u32 = 32;
/// The most a manifest may ask for, with visible consent: 64 MiB.
pub const MAX_MEMORY_PAGES: u32 = 1024;

/// An HTTP response's default cap, and the one a manifest may ask for.
pub const MAX_HTTP_RESPONSE_BYTES: u64 = 4 << 20;
pub const MAX_HTTP_RESPONSE_CONSENTED_BYTES: u64 = 16 << 20;

/// Requests in flight, per plugin and in total.
pub const MAX_HTTP_IN_FLIGHT: usize = 4;
pub const MAX_HTTP_IN_FLIGHT_TOTAL: usize = 16;

/// A plugin's storage, by default and with a manifest request.
pub const DEFAULT_STORAGE_BYTES: u64 = 1 << 20;
pub const MAX_STORAGE_BYTES: u64 = 16 << 20;

/// Timers a plugin may hold, and the shortest interval one may ask for.
pub const MAX_TIMERS: usize = 16;
pub const MIN_TIMER_INTERVAL_MS: u64 = 1_000;

/// Topics one plugin may subscribe to, and how long a topic's name may be.
///
/// A topic is the publishing plugins' own namespace, so the cap is what keeps
/// a plugin from filling the parent's registry with names nobody publishes to.
pub const MAX_TOPICS: usize = 32;
pub const MAX_TOPIC_NAME_BYTES: usize = 128;

/// The most one `plugin.call` carries in either direction, base64 aside: a call
/// is a message between two plugins, and a megabyte of it is a payload that
/// belongs in storage, not in one frame each way.
pub const MAX_CALL_BYTES: usize = 256 << 10;

/// State handed from `yf_deactivate` to the next `yf_activate`.
pub const MAX_HANDOFF_BYTES: usize = 256 << 10;

/// The helper's own total cap, before it evicts or refuses an instance.
pub const MAX_HELPER_BYTES: u64 = 256 << 20;

/// A git fetch's cap on disk, and its time limit.
pub const MAX_FETCH_BYTES: u64 = 64 << 20;
pub const MAX_FETCH_SECONDS: u64 = 120;

/// The largest icon a plugin may bring: a picture the review draws at 64×64 to
/// 256×256.
pub const MAX_ICON_BYTES: u64 = 64 << 10;

/// An instance that has made no call for this long may be evicted, unless it
/// holds a timer or a request in flight.
pub const IDLE_EVICTION_MS: u64 = 5 * 60 * 1000;

/// Strikes within [`STRIKE_WINDOW_MS`] that suspend a plugin for the session.
pub const MAX_STRIKES: u32 = 3;
pub const STRIKE_WINDOW_MS: u64 = 60 * 1000;

/// How the parent backs off when the helper exits unexpectedly.
pub const RESTART_BACKOFF_MS: [u64; 3] = [1_000, 5_000, 30_000];
/// Crashes within this window in [`MAX_CRASHES`] leave plugins off.
pub const CRASH_WINDOW_MS: u64 = 60 * 1000;
pub const MAX_CRASHES: u32 = 3;

/// Consent dialogs one plugin may have open, and denials that silence it.
pub const MAX_DENIALS_BEFORE_SILENCE: u32 = 3;

/// How long an unanswered consent dialog counts as open. Past it the question is
/// put again rather than the host being refused for good: a window closed by
/// accident is not an answer, and the alternative is a plugin that only works
/// again once someone edits its grants file.
pub const CONSENT_WINDOW_MS: u64 = 120_000;
