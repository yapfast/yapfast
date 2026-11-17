//! A forum: what it shows and what it asks for. Task C26
//! (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// A forum: what its screen draws. The window holds it as `vm.chat.forum`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Forum {}

/// A forum: what a click or a key on it asks for. Keep `#[non_exhaustive]`:
/// the app matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
