//! The expression picker above the composer, with its tabs: what it shows and
//! what it asks for. Task C18 (docs/iced/tasks/chat.md) fills it; F01's stub
//! until then.

/// The expression picker above the composer, with its tabs: what its screen
/// draws. The window holds it as `vm.chat.pickers`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pickers {}

/// The expression picker above the composer, with its tabs: what a click or a
/// key on it asks for. Keep `#[non_exhaustive]`: the app matches it with a
/// wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
