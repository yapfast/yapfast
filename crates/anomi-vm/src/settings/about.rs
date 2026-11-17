//! The About settings page, with updates: what it shows and what it asks for.
//! Task S10 (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// The About settings page, with updates: what its screen draws. The window
/// holds it as `vm.settings.about`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct About {}

/// The About settings page, with updates: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
