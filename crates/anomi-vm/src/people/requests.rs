//! The Requests screen: what it shows and what it asks for. Task P02
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// The Requests screen: what its screen draws. The window holds it as
/// `vm.people.requests`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Requests {}

/// The Requests screen: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
