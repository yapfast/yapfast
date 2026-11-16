//! The conversation beside the sidebar: its header, and below it the
//! timeline and the composer, which come with C01 and C14.

use crate::kind::Avatar;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conversation {
    pub header: Header,
}

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
