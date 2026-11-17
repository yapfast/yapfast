//! Deleting a server or handing it over: what it shows and what it asks for.
//! Task X12 (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// Deleting a server or handing it over: what its screen draws. The window
/// holds it as `vm.settings.ownership`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ownership {}

/// Deleting a server or handing it over: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
