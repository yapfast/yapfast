//! The two-factor prompt: what it shows and what it asks for. Task S11
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// The two-factor prompt: what its screen draws. The window holds it as
/// `vm.settings.mfa`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mfa {}

/// The two-factor prompt: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
