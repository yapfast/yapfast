//! The quick switcher: what it shows and what it asks for. Task N10
//! (docs/iced/tasks/shell.md) fills it; F01's stub until then.

/// The quick switcher: what its screen draws. The window holds it as
/// `vm.switcher`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Switcher {}

/// The quick switcher: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
