//! The activity cards: what it shows and what it asks for. Task P06
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// The activity cards: what its screen draws. The window holds it as
/// `vm.people.activity`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Activity {}

/// The activity cards: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
