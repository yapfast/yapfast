//! A server's audit log: what it shows and what it asks for. Task X07
//! (docs/iced/tasks/settings.md) fills it; F01's stub until then.

/// A server's audit log: what its screen draws. The window holds it as
/// `vm.settings.audit`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audit {}

/// A server's audit log: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
