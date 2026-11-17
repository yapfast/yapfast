//! Adding a friend: what it shows and what it asks for. Task P03
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// Adding a friend: what its screen draws. The window holds it as
/// `vm.people.add_friend`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddFriend {}

/// Adding a friend: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
