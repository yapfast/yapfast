//! The member list: what it shows and what it asks for. Task P05
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// The member list: what its screen draws. The window holds it as
/// `vm.people.members`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Members {}

/// The member list: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
