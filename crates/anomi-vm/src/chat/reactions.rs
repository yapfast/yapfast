//! A message's reactions, and the Reactions window: what it shows and what it
//! asks for. Task C03 (docs/iced/tasks/chat.md) fills it; F01's stub until
//! then.

/// A message's reactions, and the Reactions window: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reactions {}

/// The Reactions window: who reacted with what. The window holds it as
/// `vm.chat.reactions`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dialog {}

/// A message's reactions, and the Reactions window: what a click or a key on
/// it asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
