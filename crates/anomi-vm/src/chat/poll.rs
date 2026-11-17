//! A message's poll, and the dialog that makes one: what it shows and what it
//! asks for. Task C07 (docs/iced/tasks/chat.md) fills it; F01's stub until
//! then.

/// A message's poll, and the dialog that makes one: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poll {}

/// The dialog that makes a poll. The window holds it as `vm.chat.poll`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dialog {}

/// A message's poll, and the dialog that makes one: what a click or a key on
/// it asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
