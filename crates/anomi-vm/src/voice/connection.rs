//! A call's connection details: what it shows and what it asks for. Task V02
//! (docs/iced/tasks/voice.md) fills it; F01's stub until then.

/// A call's connection details: what its screen draws. The window holds it as
/// `vm.voice.connection`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Connection {}

/// A call's connection details: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
