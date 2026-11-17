//! The participant grid: what it shows and what it asks for. Task V04
//! (docs/iced/tasks/voice.md) fills it; F01's stub until then.

/// The participant grid: what its screen draws. The window holds it as
/// `vm.voice.stage`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stage {}

/// The participant grid: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
