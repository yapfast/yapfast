//! The conversation's header: the open place's title (with its topic or the
//! status of the person) and the buttons at its right. The Slint side: the
//! header of `ui/yapfast.slint` (`:3138`-`:3464`) and `show_header`,
//! `show_dm_tools` in `src/ui/refresh.rs`. Task N07
//! (docs/iced/tasks/shell.md).
//!
//! [`Place`] is the header as the window shows it. [`Header::Direct`] and
//! [`Header::Channel`] are the skeleton's two shapes, which the app's
//! projection still builds: a title and no buttons, until I04 builds
//! [`Header::Open`] from the core's state.

use crate::icon::Icon;
use crate::kind::{Avatar, Presence, Status};
use crate::menu::Spot;
use crate::text::Text;

/// What the header shows of the open conversation.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Header {
    /// Nothing is open.
    #[default]
    None,
    /// A DM or a group: its avatar and name, no buttons.
    Direct { name: String, avatar: Avatar },
    /// A server's channel: its name and topic, no buttons.
    Channel { name: String, topic: String },
    /// Any place, with its buttons.
    Open(Box<Place>),
}

/// A place's title and its buttons.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub title: Title,
    pub tools: Tools,
}

/// Who or what the header names.
#[derive(Clone, Debug, PartialEq)]
pub enum Title {
    Direct(Direct),
    Channel(Channel),
    Screen(Screen),
    /// With no room selected and none of Friends, Requests or Browse
    /// Channels open: the sidebar's title, "Direct messages" or the server's
    /// name, in `Theme.secondary` (`ui/yapfast.slint:3233`).
    Sidebar(String),
}

/// A DM or a group (`room-direct`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Direct {
    pub name: String,
    pub avatar: Avatar,
    /// The dot on the avatar (`room-presence`).
    pub presence: Presence,
    /// A one-to-one DM: the avatar and name are a button that opens the
    /// person's card (`room-person`).
    pub person: bool,
    /// What they are up to, in a bubble beside the name
    /// (`room-activity`).
    pub activity: Option<Status>,
}

/// A server channel, a thread or a forum post.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// The channel's kind: `Hash`, a forum's, a voice channel's...
    /// (`room-icon`).
    pub icon: Icon,
    pub name: String,
    /// A thread follows the channel it belongs to, which leads back to it.
    pub parent: Option<Parent>,
    /// The topic: on one line after a dot, in full once clicked
    /// (`has-topic`).
    pub topic: Option<Text>,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            icon: Icon::Hash,
            name: String::new(),
            parent: None,
            topic: None,
        }
    }
}

/// The channel a thread is in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parent {
    pub name: String,
    /// A forum, drawn with its own icon.
    pub forum: bool,
}

/// What stands in for a place: the screens that have a title of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Friends, with its Online and All tabs and the Add Friend button
    /// (`friends-open`, `friends-all`).
    Friends {
        all: bool,
    },
    Requests,
    Browse,
}

/// The header's buttons, left to right as the Slint header lays them; each
/// is shown only where `Some` or true.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tools {
    /// Befriending the other person of a DM (`dm-friend`).
    pub friend: Friend,
    /// A voice channel's chat: whether the chat is the one showing
    /// (`has-voice-chat`, `voice-chat`).
    pub voice_chat: Option<bool>,
    /// A voice and a video call (`can-call`).
    pub call: Option<Call>,
    /// The threads popover; whether it is open (`can-list-threads`).
    pub threads: Option<bool>,
    /// The notification menu; whether the place is muted (`can-notify`).
    pub notify: Option<bool>,
    /// The pins popover; whether it is open (`can-pin`).
    pub pins: Option<bool>,
    /// The friends picker (`can-add-people`).
    pub add_people: Option<AddPeople>,
    /// The member list (`members-available`).
    pub members: Option<Members>,
    /// A thread's menu (`room-parent != ""`).
    pub more: bool,
    /// A DM's profile panel; whether it shows (`profile-available`).
    pub profile: Option<bool>,
    /// The search field has its place at the right (`can-search`); C21
    /// draws it and `anomi_screens::header::with_search` places it.
    pub search: bool,
    /// The members' count in words where there is no list to open
    /// (`members`); empty for none.
    pub count: String,
    /// The conversation is narrower than 560 px: the parts sit 4 px apart,
    /// not 10, and the activity bubble gives way when it would get less than
    /// its width or 96 px (`:3146`, `:3284`). The projection sets it from the
    /// conversation's width.
    pub compact: bool,
}

/// Whether and how a DM's other person can be befriended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Friend {
    #[default]
    None,
    /// "Add Friend".
    Add,
    /// They asked: "Accept Friend Request".
    Accept,
}

/// The call buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Call {
    /// Someone is in it, so the button joins and no video call starts
    /// (`call-going`).
    pub going: bool,
    /// Calling is possible now (`can-join`).
    pub can: bool,
}

/// The friends picker's button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AddPeople {
    pub open: bool,
    /// The DM is one-to-one, so adding makes a group.
    pub one_to_one: bool,
}

/// The member list's button.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Members {
    /// The list shows (`members-showing`).
    pub showing: bool,
    /// A DM's button says how many, "3 members" (`members`).
    pub label: String,
}

/// What the header asks for. Keep `#[non_exhaustive]`: the app matches it
/// with a wildcard until its I task handles every variant. Those that take
/// a [`Spot`] open something under their button; the others toggle or act.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A one-to-one DM's person: their card under the button.
    Profile(Spot),
    /// A thread's channel: back to it.
    Parent,
    /// The topic in full, under its line (toggles).
    Topic(Spot),
    /// Add the other person as a friend, or accept their request.
    Befriend,
    /// Show a voice channel's chat (`true`) or its stage (`false`).
    VoiceChat(bool),
    Call,
    VideoCall,
    /// The threads popover (toggles).
    Threads(Spot),
    /// The notification menu.
    Notifications(Spot),
    /// The pins popover (toggles).
    Pins(Spot),
    /// The friends picker (toggles).
    AddPeople(Spot),
    /// Show or hide the member list.
    Members,
    /// A thread's menu.
    More(Spot),
    /// Show or hide a DM's profile panel.
    ProfilePanel,
    /// The Friends screen's tabs: all of them, or those online.
    FriendsAll(bool),
    /// The Add Friend dialog.
    AddFriend,
    /// A link of the topic in full was clicked (`open-link`).
    Link(crate::text::Link),
    /// Close the popover the header opened (the topic's).
    Close,
}
