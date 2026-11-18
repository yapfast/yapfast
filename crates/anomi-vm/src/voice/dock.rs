//! The voice dock: what it shows and what it asks for. Task V01
//! (docs/iced/tasks/voice.md). The Slint side is `VoiceDock` (`ui/voice.slint:393`)
//! over the properties `show_voice` fills (`src/ui/refresh.rs:4592`) and the
//! call's timer (`src/ui/call_timer.rs`).
//!
//! The dock tops the account card while a call goes or failed
//! (`Shell::voice_dock`). Its text is made by the projection, translated
//! there; the elapsed time is too: the app counts it (once a second, only
//! while the window is in front, as `CallTimer` does) and hands the dock
//! [`elapsed`] of it, so the dock itself never ticks.

use std::time::Duration;

use crate::menu::Spot;

/// How the call is going, which tints its status and its icon
/// (`voice-tone`, `ui/yapfast.slint:2004`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    /// Joining, reconnecting or leaving: the accent.
    #[default]
    Working,
    /// Connected: the live colour.
    Connected,
    /// Disconnected: the danger colour.
    Disconnected,
}

/// The connection's quality, which colours the signal icon and gives it
/// bars (`SignalIcon`, `ui/voice.slint:136`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Signal {
    #[default]
    Unknown,
    Fine,
    Average,
    Bad,
}

/// The voice dock: what its screen draws. The window holds it as
/// `vm.voice.dock`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dock {
    /// "Voice connected", "Joining voice...": what the connection is doing.
    pub status: String,
    /// How long the call has lasted, shown as [`elapsed`] says, read as it
    /// draws; none before it connects.
    pub time: Option<crate::kind::Elapsed>,
    /// The channel's or the DM's name and how the call is secured: the line
    /// that opens the call.
    pub detail: String,
    /// The call's place, as "Streaming to {}" names it.
    pub name: String,
    pub tone: Tone,
    /// A call is going (not only a failure to show).
    pub active: bool,
    /// Connected or reconnecting: what needs a connection is enabled.
    pub ready: bool,
    /// Leaving: the hang-up waits.
    pub leaving: bool,
    /// The connection's quality once it was measured: the signal icon and
    /// the details it opens. `None` draws the speaker in the status's tone.
    pub signal: Option<Signal>,
    /// The ping, as the status's tooltip; empty draws none.
    pub ping: String,
    /// Something to watch for ("Your connection is poor").
    pub warning: String,
    /// What went wrong; shown instead of the warning.
    pub error: String,
    pub can_share: bool,
    pub sharing: bool,
    /// The stream is live, not choosing or starting.
    pub share_live: bool,
    /// "Choose what to share", "Starting your stream...": before it's live.
    pub share_status: String,
    /// How many watch this account's stream.
    pub share_watchers: usize,
    pub share_sound_on: bool,
    pub can_film: bool,
    pub camera_on: bool,
    /// The camera is on its way, until its first picture.
    pub camera_starting: bool,
    /// The call has Discord's soundboard.
    pub can_soundboard: bool,
    pub noise_on: bool,
    /// Noise suppression can be turned on and off from the call (Discord's).
    pub can_noise: bool,
}

/// The call's length as the dock shows it: `5:07`, `1:02:09` past an hour
/// (`elapsed`, `src/ui/call_timer.rs:67`).
pub fn elapsed(time: Duration) -> String {
    let seconds = time.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The voice dock: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Hang up.
    Leave,
    /// Put away what went wrong; with no call, closes the dock.
    DismissError,
    ToggleCamera,
    /// Stop sharing the screen (the cross beside "Your screen").
    StopSharing,
    ToggleStreamSound,
    ToggleNoise,
    /// Sharing starts from a menu opened at the button, and while sharing
    /// the same menu switches what's shared.
    OpenShareMenu(Spot),
    /// The soundboard, opened at its button.
    OpenSoundboard(Spot),
    /// The connection's details, from the status or the signal icon.
    OpenLink(Spot),
    /// The pointer came to the status: read the ping fresh (`CallLink.read`).
    ReadLink,
    /// Show the call's channel or DM.
    OpenCall,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_counts_minutes_then_hours() {
        assert_eq!(elapsed(Duration::ZERO), "0:00");
        assert_eq!(elapsed(Duration::from_secs(125)), "2:05");
        assert_eq!(elapsed(Duration::from_secs(3_729)), "1:02:09");
    }
}
