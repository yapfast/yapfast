//! A Community server's Browse Channels: what it shows and what it asks for.
//! Task N14 (docs/iced/tasks/shell.md) fills it; F01's stub until then.

/// A Community server's Browse Channels: what its screen draws. The window
/// holds it as `vm.browse`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Browse {}

/// A Community server's Browse Channels: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
