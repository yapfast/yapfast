//! A server's Safety settings: what it shows and what it asks for. Task X05
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// A server's Safety settings: what its screen draws. The window holds it as
/// `vm.settings.safety`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Safety {}

/// A server's Safety settings: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
