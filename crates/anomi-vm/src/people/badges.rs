//! A profile's badges: what it shows and what it asks for. Task P08
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// A profile's badges: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Badges {}

/// A profile's badges: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
