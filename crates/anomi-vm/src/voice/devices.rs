//! The device menus and the input level: what it shows and what it asks for.
//! Task V09 (docs/iced/tasks/voice.md) fills it; F01's stub until then.

/// The device menus and the input level: what its screen draws. The window
/// holds it as `vm.voice.devices`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Devices {}

/// The device menus and the input level: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
