//! The context menu open over the window, with its rows: what it shows and
//! what it asks for. Task F06 (docs/iced/tasks/foundation.md) fills it; F01's
//! stub until then.

/// The context menu open over the window, with its rows: what its screen
/// draws. The window holds it as `vm.menu`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {}

/// The context menu open over the window, with its rows: what a click or a
/// key on it asks for. Keep `#[non_exhaustive]`: the app matches it with a
/// wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
