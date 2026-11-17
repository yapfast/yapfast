//! The message menu: what it shows and what it asks for. Task C12
//! (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// The message menu: what its screen draws. The window holds it as
/// `vm.chat.menu`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {}

/// The message menu: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
