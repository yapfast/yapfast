//! The conversation column beside the sidebar: the header
//! ([`crate::header`]) above what the open place shows, and the panels
//! docked at its right. The Slint side: the conversation column of
//! `YapfastWindow` (`ui/yapfast.slint:3135`-`:4493`), whose parts are the
//! areas' own view-models; this says which of them show and where. What
//! the column's own parts ask for (the forum post's bar) is the window's
//! [`crate::shell::Intent`]. The Slint column's graphics note
//! (`graphics-notice`, `:3469`) has no part here: it says the GPU failed,
//! and the iced interface draws with tiny-skia alone.

/// What the conversation column shows beyond its areas' own view-models.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conversation {
    /// What fills the column under the header.
    pub body: Body,
    /// A call above the messages, or a voice channel's own.
    pub call: Call,
    /// While in the call, the line over its stage: who is being called,
    /// that nobody else is in it yet, or nothing while it simply goes on
    /// (`voice-description`, `:3517`).
    pub description: String,
    /// What is docked at the right.
    pub beside: Beside,
    /// The local preview's sample data: a line says so at the top
    /// (`preview`, `:3453`).
    pub preview: bool,
    /// A forum post open as the conversation: the bar to react to it,
    /// follow it and copy its link (`:3554`).
    pub forum_post: Option<ForumPost>,
}

/// What fills the column under the header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Body {
    /// No place chosen: the timeline says what to do (C01's
    /// `NoConversation`).
    #[default]
    Nothing,
    /// A conversation: its timeline above its composer.
    Messages,
    /// Friends, with Active Now beside them in a wide column
    /// (`friends-open`, `:4066`).
    Friends,
    /// Message requests (`requests-open`, `:4057`).
    Requests,
    /// A Community server's channels to browse (`browse-open`, `:4086`).
    Browse,
    /// A forum's posts in place of its messages (`room-forum`, `:3616`).
    Forum,
    /// A voice channel, not its chat: its stage, or the way to join, fills
    /// the column (`voice-panel`, `:554`).
    Voice,
}

impl Body {
    /// The composer shows under it (`:4140`).
    pub fn composes(self) -> bool {
        self == Self::Messages
    }
}

/// A call shown with the body (`voice-panel || call-shown`, `:3473`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Call {
    #[default]
    None,
    /// Not in it: the way to join a voice channel (`JoinVoice`), or above a
    /// DM's messages the call going on there (`CallInvite`), V07's `join`.
    Elsewhere,
    /// In it: [`Conversation::description`] over its stage, V04's
    /// (`voice-here`).
    Here {
        /// A tile is focused: the call takes more of a DM's room
        /// (`call-focus >= 0`).
        focused: bool,
    },
}

/// What the column wants docked at its right; the shell decides what fits
/// (`anomi_screens::shell::Docking`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Beside {
    /// Search results, in place of every other panel (`search-open`).
    pub search: bool,
    /// The thread beside a channel, or the form that starts one, in place
    /// of the member list and the profile (`aside-open`, `new-thread-open`).
    pub thread: Option<Aside>,
    /// The member list: the place has one and the person keeps it shown
    /// (`members-available && member-list`).
    pub members: bool,
    /// A DM's profile: the DM has one and the person keeps it shown
    /// (`profile-available && profile-panel && panel-profile-shown`).
    pub profile: bool,
}

/// What shows beside a channel in place of its member list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aside {
    /// A thread, C24's panel.
    Thread,
    /// The form that starts one, C25's.
    NewThread,
}

/// A forum post open as the conversation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForumPost {
    /// Its first message can take a reaction (`forum-post-item`).
    pub reacts: bool,
    /// Whether the person follows it; `None` where they can't
    /// (`forum-post-follow`).
    pub following: Option<bool>,
}
