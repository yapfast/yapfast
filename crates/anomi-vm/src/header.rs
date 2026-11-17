//! The conversation's header: the open place's title and topic, and its
//! buttons. The Slint side: the header of `ui/yapfast.slint` and
//! `show_header` in `src/ui/refresh.rs`. Task N07 (docs/iced/tasks/shell.md)
//! takes it further; the skeleton shows a DM's avatar and name or a
//! channel's name and topic.

use crate::kind::Avatar;

/// What the header shows of the open conversation.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Header {
    /// Nothing is open.
    #[default]
    None,
    /// A DM or a group: its avatar and name.
    Direct { name: String, avatar: Avatar },
    /// A server's channel: its name and topic.
    Channel { name: String, topic: String },
}

/// What the header asks for. Keep `#[non_exhaustive]`: the app matches it
/// with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
