//! The join dialog, for an invite's server or adding one: what it shows and
//! what it asks for. Task N12 (docs/iced/tasks/shell.md) fills it; F01's stub
//! until then.

/// The join dialog, for an invite's server or adding one: what its screen
/// draws. The window holds it as `vm.places.join`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Join {}

/// The join dialog, for an invite's server or adding one: what a click or a
/// key on it asks for. Keep `#[non_exhaustive]`: the app matches it with a
/// wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
