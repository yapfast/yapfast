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

/// State handed from `yf_deactivate` to the next `yf_activate`.
pub const MAX_HANDOFF_BYTES: usize = 256 << 10;

/// The helper's own total cap, before it evicts or refuses an instance.
pub const MAX_HELPER_BYTES: u64 = 256 << 20;

/// A git fetch's cap on disk, and its time limit.
pub const MAX_FETCH_BYTES: u64 = 64 << 20;
pub const MAX_FETCH_SECONDS: u64 = 120;

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

/// A rule list edited in a plugin's settings, by entries.
pub const MAX_SETTINGS_LIST_ENTRIES: usize = 256;
