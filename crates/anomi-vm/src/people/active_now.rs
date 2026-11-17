//! The Active Now column: what it shows and what it asks for. Task P04
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// The Active Now column: what its screen draws. The window holds it as
/// `vm.people.active_now`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveNow {}

/// The Active Now column: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
