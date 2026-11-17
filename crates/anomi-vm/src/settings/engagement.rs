//! A server's Engagement page: what it shows and what it asks for. Task X02
//! (docs/iced/tasks/settings.md). The Slint side: `EngagementSection`
//! (`ui/server_settings.slint:159`) and `EngagementPrompt`.

/// A switch of the system messages (Slint numbered them 0 to 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    /// A random welcome message when someone joins.
    Welcome,
    /// A prompt to reply to it with a sticker.
    WelcomeSticker,
    /// A message when someone boosts.
    Boosted,
    /// Tips for server setup.
    SetupTips,
}

/// A server's Engagement page: what its screen draws. The window holds it as
/// `vm.settings.engagement`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Engagement {
    /// "No System Messages", then the server's text channels.
    pub system_channels: Vec<String>,
    /// The one chosen, by index; 0 turns system messages off.
    pub system_channel: usize,
    pub welcome: bool,
    pub welcome_sticker: bool,
    pub boosted: bool,
    pub setup_tips: bool,
    /// 0 all messages, 1 only @mentions.
    pub notifications: usize,
    /// "No Inactive Channel", then the server's voice channels.
    pub afk_channels: Vec<String>,
    pub afk_channel: usize,
    /// Which of the timeouts, 1 minute to 1 hour.
    pub afk_timeout: usize,
    /// Discord is taking a change: the controls go still.
    pub saving: bool,
}

/// A server's Engagement page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A switch flipped (`toggle(n)`).
    Toggle(Switch),
    /// The system channel chosen, by index (`choose(0, i)`).
    SystemChannel(usize),
    /// The default notifications chosen (`choose(1, i)`).
    Notifications(usize),
    /// The inactive channel chosen (`choose(2, i)`).
    AfkChannel(usize),
    /// The inactive timeout chosen (`choose(3, i)`).
    AfkTimeout(usize),
}
