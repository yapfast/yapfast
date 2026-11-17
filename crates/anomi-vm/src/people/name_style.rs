//! The name style window: what it shows and what it asks for. Task P16
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// The name style window: what its screen draws. The window holds it as
/// `vm.people.name_style`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NameStyle {}

/// The name style window: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
