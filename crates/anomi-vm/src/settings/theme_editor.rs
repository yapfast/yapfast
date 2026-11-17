//! The theme editor: what it shows and what it asks for. Task S07
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// The theme editor: what its screen draws. The window holds it as
/// `vm.settings.theme_editor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeEditor {}

/// The theme editor: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
