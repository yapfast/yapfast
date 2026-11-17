//! The timeline: what it shows and what it asks for. Task C01
//! (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// The timeline: what its screen draws. The window holds it as
/// `vm.chat.timeline`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {}

/// The timeline: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
