//! Leaving a server, and the move-sync confirmation: what it shows and what
//! it asks for. Task N13 (docs/iced/tasks/shell.md) fills it; F01's stub
//! until then.

/// Leaving a server, and the move-sync confirmation: what its screen draws.
/// The window holds it as `vm.places.leave`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Leave {}

/// Leaving a server, and the move-sync confirmation: what a click or a key on
/// it asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
