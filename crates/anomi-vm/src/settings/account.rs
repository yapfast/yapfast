//! The Account settings page: what it shows and what it asks for. Task S02
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// The Account settings page: what its screen draws. The window holds it as
/// `vm.settings.account`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {}

/// The Account settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
