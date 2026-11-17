//! An incoming call: what it shows and what it asks for. Task V08
//! (docs/iced/tasks/voice.md) fills it; F01's stub until then.

/// An incoming call: what its screen draws. The window holds it as
/// `vm.voice.incoming`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Incoming {}

/// An incoming call: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
