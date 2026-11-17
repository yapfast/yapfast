//! The Keybinds settings page: what it shows and what it asks for. Task S09
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// The Keybinds settings page: what its screen draws. The window holds it as
/// `vm.settings.keybinds`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keybinds {}

/// The Keybinds settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
