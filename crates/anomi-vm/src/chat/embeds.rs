//! A message's embeds: what it shows and what it asks for. Task C05
//! (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// A message's embeds: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Embeds {}

/// A message's embeds: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
