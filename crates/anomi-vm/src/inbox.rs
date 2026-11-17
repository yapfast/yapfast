//! The inbox popout of mentions and unreads: what it shows and what it asks
//! for. Task N08 (docs/iced/tasks/shell.md) fills it; F01's stub until then.

/// The inbox popout of mentions and unreads: what its screen draws. The
/// window holds it as `vm.inbox`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inbox {}

/// The inbox popout of mentions and unreads: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
