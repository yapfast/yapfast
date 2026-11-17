//! A server's settings frame and its profile page: what it shows and what it
//! asks for. Task X01 (docs/iced/tasks/settings.md) fills it; F01's stub
//! until then.

/// A server's settings frame and its profile page: what its screen draws. The
/// window holds it as `vm.settings.server`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {}

/// A server's settings frame and its profile page: what a click or a key on
/// it asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
