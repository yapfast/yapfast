//! The window's own frame, with its title bar, caption buttons and resize
//! edges: what it shows and what it asks for. Task F18
//! (docs/iced/tasks/foundation.md) fills it; F01's stub until then.

/// The window's own frame, with its title bar, caption buttons and resize
/// edges: what its screen draws. The window holds it as `vm.chrome`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chrome {}

/// The window's own frame, with its title bar, caption buttons and resize
/// edges: what a click or a key on it asks for. Keep `#[non_exhaustive]`: the
/// app matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
