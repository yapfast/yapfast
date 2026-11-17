//! The Sounds settings page: what it shows and what it asks for. Task S05
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// The Sounds settings page: what its screen draws. The window holds it as
/// `vm.settings.sounds`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sounds {}

/// The Sounds settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
