//! The composer's frame: what it shows and what it asks for. Task C14
//! (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// The composer's frame: what its screen draws. The window holds it as
/// `vm.chat.composer`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Composer {
    /// Mentions, channels and emoji offered as one types: C15. Wired by F01; keep it.
    pub suggestions: super::suggestions::Suggestions,
    /// Slash commands offered as one types: C16. Wired by F01; keep it.
    pub commands: super::commands::Commands,
}

/// The composer's frame: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
