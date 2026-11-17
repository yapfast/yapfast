//! A thread beside its channel: what it shows and what it asks for. Task C24
//! (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// A thread beside its channel: what its screen draws. The window holds it as
/// `vm.chat.thread_panel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadPanel {}

/// A thread beside its channel: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
