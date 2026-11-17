//! The confirmation before leaving for a link: what it shows and what it asks
//! for. Task N13 (docs/iced/tasks/shell.md) fills it; F01's stub until then.

/// The confirmation before leaving for a link: what its screen draws. The
/// window holds it as `vm.places.leaving_link`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeavingLink {}

/// The confirmation before leaving for a link: what a click or a key on it
/// asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
