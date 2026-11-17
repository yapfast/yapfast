//! The composer's slash commands: what it shows and what it asks for. Task
//! C16 (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// The composer's slash commands: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Commands {}

/// The composer's slash commands: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
