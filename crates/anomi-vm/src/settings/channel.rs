//! A channel's settings frame and its overview: what it shows and what it
//! asks for. Task X13 (docs/iced/tasks/settings.md) fills it; F01's stub
//! until then.

/// A channel's settings frame and its overview: what its screen draws. The
/// window holds it as `vm.settings.channel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channel {}

/// A channel's settings frame and its overview: what a click or a key on it
/// asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
