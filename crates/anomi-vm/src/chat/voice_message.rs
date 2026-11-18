//! A voice message's player: what it shows and what it asks for. Task C11
//! (docs/iced/tasks/chat.md); ports `FileCard`'s pill (`ui/messages.slint`)
//! and `show_voice_message` (`src/ui/refresh.rs`). Whether its speed and
//! mute are out is the screen's own (the pointer or the keyboard on it).

/// A voice message's player: what its screen draws. The player is the
/// app's one (`VoicePlayer`); `active` says it is this message that plays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoiceMessage {
    /// The message's item, which the intents name.
    pub item: u64,
    /// How long it is, as shown before it starts.
    pub duration: String,
    /// One level from 0 to 1 for each bar of the waveform.
    pub waveform: Vec<f32>,
    /// This message is the one the player holds.
    pub active: bool,
    pub playing: bool,
    /// How far it has played, 0 to 1.
    pub progress: f32,
    /// What is left once it started, empty before.
    pub time: String,
    /// Why it can't play, shown in the time's place.
    pub error: String,
    /// The speed's label, `1.5x`.
    pub speed: String,
    /// The player's volume, 0 to 1, and whether it is muted.
    pub volume: f32,
    pub muted: bool,
    /// A preview, or a message not yet delivered, can't be played.
    pub enabled: bool,
}

impl VoiceMessage {
    /// The time shown: the error, what is left, or its length.
    pub fn shown_time(&self) -> &str {
        if self.active && !self.error.is_empty() {
            &self.error
        } else if self.active && !self.time.is_empty() {
            &self.time
        } else {
            &self.duration
        }
    }

    /// How far it has played, nothing while another message plays.
    pub fn played(&self) -> f32 {
        if self.active {
            self.progress.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn is_playing(&self) -> bool {
        self.active && self.playing
    }

    /// The volume the player shows: a muted one is 0.
    pub fn shown_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume }
    }
}

/// A voice message's player: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The round button: play it, or pause it (`VoicePlayer.toggle`).
    Toggle { item: u64 },
    /// A press or drag on the waveform: move to `at`, 0 to 1
    /// (`VoicePlayer.seek`).
    Seek { item: u64, at: f32 },
    /// The speed button: the next speed (`VoicePlayer.cycle-speed`).
    CycleSpeed,
    /// The speaker: mute or unmute (`VoicePlayer.mute`).
    Mute(bool),
    /// The wheel: the volume, 0 to 1 (`VoicePlayer.change-volume`).
    Volume(f32),
}
