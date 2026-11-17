//! Making a channel or a category: what it shows and what it asks for. Task
//! X15 (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// Making a channel or a category: what its screen draws. The window holds it
/// as `vm.settings.create_channel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateChannel {}

/// Making a channel or a category: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
