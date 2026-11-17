//! A server's Invites settings: what it shows and what it asks for. Task X04
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// A server's Invites settings: what its screen draws. The window holds it as
/// `vm.settings.invites`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Invites {}

/// A server's Invites settings: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
