//! The conversation column beside the sidebar: the header ([`crate::header`])
//! above what the open place shows (the timeline and composer, Friends, a
//! forum, a call). Task N01 (docs/iced/tasks/shell.md) decides what it
//! holds; the skeleton's header moved to [`crate::header`].

/// What the conversation column shows beyond its areas' own view-models.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conversation {}
