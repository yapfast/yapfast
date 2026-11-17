//! A participant's menu: what it shows and what it asks for. Task V11
//! (docs/iced/tasks/voice.md) fills it; F01's stub until then.

/// A participant's menu: what its screen draws. The window holds it as
/// `vm.voice.participant_menu`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticipantMenu {}

/// A participant's menu: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
