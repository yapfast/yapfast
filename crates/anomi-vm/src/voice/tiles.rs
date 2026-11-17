//! A call's tiles: what it shows and what it asks for. Task V03
//! (docs/iced/tasks/voice.md) fills it; F01's stub until then.

/// A call's tiles: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {}

/// A call's tiles: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
