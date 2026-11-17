//! A video in a message, and full screen: what it shows and what it asks for.
//! Task C09 (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// A video in a message, and full screen: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Video {}

/// A video played full screen. The window holds it as `vm.chat.video`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FullScreen {}

/// A video in a message, and full screen: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
