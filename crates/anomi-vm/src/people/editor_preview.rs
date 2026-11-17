//! The profile editor's preview and colours: what it shows and what it asks
//! for. Task P15 (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// The profile editor's preview and colours: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorPreview {}

/// The profile editor's preview and colours: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
