//! The server's header at the top of its channel list: its name, banner and
//! menu. The Slint side: `ServerHeader` (`ui/rail.slint:946`). Task N05
//! (docs/iced/tasks/shell.md) takes it further; the skeleton shows the name.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerHeader {
    pub name: String,
}

/// What the server's header asks for. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
