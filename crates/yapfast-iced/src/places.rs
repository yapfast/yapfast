//! The menus of places: a server's (from its name over the channels), a
//! channel's, a category's or a DM's (a right click on its row), as the
//! Slint client builds them from the core's `menu_choices`: Discord's
//! wording, a line between groups, no icons but Copy ID's, and Copy ID
//! only with Developer Mode on. Choices this client can't act on yet (the
//! dialogs and settings still to come) show greyed.

use iced::{Point, Task, widget::operation};
use yapfast_core::application::{Action, MenuChoice, network_id, network_link};

use crate::{
    app::{App, Message, NOBODY},
    menus::{Menu, MenuAct, MenuRow},
    pictures::Icon,
};

impl App {
    /// Opens the menu of the place `id` at `at`: the server's header menu
    /// when `header`.
    pub fn open_place_menu(&mut self, id: &str, at: Point, header: bool) -> Task<Message> {
        let app = &self.client.app;
        let state = &app.state;
        let mut choices = if header {
            app.header_choices(id)
        } else {
            app.menu_choices(id)
        };
        let server = state.servers.iter().any(|server| server.id == id);
        let category = !server && state.is_category(id);
        let voice = state.rooms.iter().any(|room| room.id == id && room.voice);
        // A conversation's and a server's menu starts with Mark as read.
        if !header
            && !state.preview
            && (server || state.rooms.iter().any(|room| room.id == id))
            && !choices.contains(&MenuChoice::MarkRead)
        {
            choices.insert(0, MenuChoice::MarkRead);
        }
        let developer = self.layout.developer_mode;
        let mut group = None;
        let rows: Vec<MenuRow> = choices
            .into_iter()
            .filter(|choice| developer || *choice != MenuChoice::CopyId)
            .map(|choice| {
                let label = match choice {
                    MenuChoice::Invite if server => "Invite to server",
                    MenuChoice::CreateChannel if server => "Create channel",
                    MenuChoice::CreateChannel if voice => "Create voice channel",
                    MenuChoice::EditChannel if category => "Edit category",
                    MenuChoice::DeleteChannel if category => "Delete category",
                    choice => label(choice),
                };
                let starts = group_of(choice);
                let separated = group.replace(starts).is_some_and(|last| last != starts);
                MenuRow {
                    act: MenuAct::Place(choice),
                    label: label.into(),
                    icon: (choice == MenuChoice::CopyId).then_some(Icon::Id),
                    danger: matches!(
                        choice,
                        MenuChoice::DeleteChannel
                            | MenuChoice::LeaveServer
                            | MenuChoice::LeaveGroup
                    ),
                    separated,
                    disabled: !acts(choice),
                    subtitle: None,
                    check: crate::controls::Check::None,
                }
            })
            .collect();
        if rows.is_empty() {
            return Task::none();
        }
        self.menu = Some(Menu {
            place: Some(id.to_owned()),
            header,
            ..Menu::new(at, rows).width(crate::menus::FIELD_WIDTH)
        });
        operation::focus(NOBODY)
    }

    /// A place's menu row was chosen.
    pub fn choose_place(&mut self, place: String, choice: MenuChoice) -> Task<Message> {
        let action = match choice {
            MenuChoice::MarkRead => Action::MarkRead(place),
            MenuChoice::JoinVoice => Action::RequestJoinVoice(place),
            MenuChoice::OpenChat => Action::OpenChat(place),
            MenuChoice::Profile => Action::ShowDmProfile(place),
            MenuChoice::Pin | MenuChoice::Unpin => Action::Pin {
                room: place,
                pinned: choice == MenuChoice::Pin,
            },
            MenuChoice::CloseDm => Action::CloseDm(place),
            MenuChoice::EditServerProfile => Action::EditServerProfile(place),
            MenuChoice::CopyLink => return iced::clipboard::write(network_link(&place)),
            MenuChoice::CopyId => return iced::clipboard::write(network_id(&place).to_owned()),
            _ => return Task::none(),
        };
        self.client.dispatch(action);
        self.keep_pins();
        self.sync()
    }
}

/// Whether this client acts on a choice yet; the rest show greyed.
fn acts(choice: MenuChoice) -> bool {
    matches!(
        choice,
        MenuChoice::MarkRead
            | MenuChoice::JoinVoice
            | MenuChoice::OpenChat
            | MenuChoice::Profile
            | MenuChoice::Pin
            | MenuChoice::Unpin
            | MenuChoice::CloseDm
            | MenuChoice::EditServerProfile
            | MenuChoice::CopyLink
            | MenuChoice::CopyId
    )
}

/// Which group a choice is in, as the Slint client draws lines between
/// them: what it does; how this account stands with someone;
/// notifications; copying and the channel list; roles; moderation; a
/// channel's management; deleting or leaving.
pub fn group_of(choice: MenuChoice) -> u8 {
    match choice {
        MenuChoice::InviteToServer
        | MenuChoice::AddFriend
        | MenuChoice::AcceptFriend
        | MenuChoice::CancelFriendRequest
        | MenuChoice::Unfriend
        | MenuChoice::Ignore
        | MenuChoice::Unignore
        | MenuChoice::Block
        | MenuChoice::Unblock => 1,
        MenuChoice::Mute
        | MenuChoice::Unmute
        | MenuChoice::ChannelMute
        | MenuChoice::ChannelNotifications
        | MenuChoice::MuteServer
        | MenuChoice::UnmuteServer
        | MenuChoice::ServerNotifications
        | MenuChoice::ShowAllChannels
        | MenuChoice::HideMuted => 2,
        MenuChoice::CopyLink
        | MenuChoice::CopyId
        | MenuChoice::JoinThread
        | MenuChoice::LeaveThread
        | MenuChoice::Invite
        | MenuChoice::PinChannel
        | MenuChoice::UnpinChannel
        | MenuChoice::AddToChannelList
        | MenuChoice::RemoveFromChannelList
        | MenuChoice::UnfollowCategory => 3,
        MenuChoice::ServerNickname | MenuChoice::Roles | MenuChoice::MoveTo => 4,
        MenuChoice::Apps => 10,
        MenuChoice::LocalMute
        | MenuChoice::SoundboardMute
        | MenuChoice::SelfMute
        | MenuChoice::SelfDeafen => 8,
        MenuChoice::ServerMute
        | MenuChoice::ServerDeafen
        | MenuChoice::Disconnect
        | MenuChoice::Timeout
        | MenuChoice::RemoveTimeout
        | MenuChoice::Kick
        | MenuChoice::Ban => 5,
        MenuChoice::TransferOwnership => 9,
        MenuChoice::ServerSettings
        | MenuChoice::EditServerProfile
        | MenuChoice::CreateChannel
        | MenuChoice::CreateCategory
        | MenuChoice::EditChannel
        | MenuChoice::DuplicateChannel => 6,
        MenuChoice::DeleteChannel | MenuChoice::LeaveServer | MenuChoice::LeaveGroup => 7,
        _ => 0,
    }
}

/// What a menu row says for a choice, in the Slint client's words.
pub fn label(choice: MenuChoice) -> &'static str {
    match choice {
        MenuChoice::MarkRead => "Mark as read",
        MenuChoice::JoinVoice => "Join voice",
        MenuChoice::OpenChat => "Open chat",
        MenuChoice::Profile => "Profile",
        MenuChoice::Message => "Message",
        MenuChoice::AddFriend => "Add friend",
        MenuChoice::AcceptFriend => "Accept friend request",
        MenuChoice::CancelFriendRequest => "Cancel friend request",
        MenuChoice::Pin => "Pin",
        MenuChoice::Unpin => "Unpin",
        MenuChoice::Call => "Start call",
        MenuChoice::AddNote => "Add note",
        MenuChoice::AddNickname => "Add friend nickname",
        MenuChoice::EditNickname => "Change friend nickname",
        MenuChoice::CloseDm => "Close DM",
        MenuChoice::LeaveGroup => "Leave group",
        MenuChoice::InviteToServer => "Invite to server",
        MenuChoice::Apps => "Apps",
        MenuChoice::Unfriend => "Remove friend",
        MenuChoice::Ignore => "Ignore",
        MenuChoice::Unignore => "Stop ignoring",
        MenuChoice::Block => "Block",
        MenuChoice::Unblock => "Unblock",
        MenuChoice::Mute => "Mute",
        MenuChoice::Unmute => "Unmute",
        MenuChoice::CopyLink => "Copy link",
        MenuChoice::CopyId => "Copy ID",
        MenuChoice::Mention => "Mention",
        MenuChoice::ServerNickname => "Change nickname",
        MenuChoice::Roles => "Roles",
        MenuChoice::RemoveTimeout => "Remove timeout",
        MenuChoice::Timeout => "Time out",
        MenuChoice::Kick => "Kick",
        MenuChoice::Ban => "Ban",
        MenuChoice::TransferOwnership => "Transfer Ownership",
        MenuChoice::LocalMute | MenuChoice::SelfMute => "Mute",
        MenuChoice::SoundboardMute => "Mute Soundboard",
        MenuChoice::SelfDeafen => "Deafen",
        MenuChoice::MoveTo => "Move to",
        MenuChoice::ServerMute => "Server mute",
        MenuChoice::ServerDeafen => "Server deafen",
        MenuChoice::Disconnect => "Disconnect",
        MenuChoice::ChannelMute => "Mute",
        MenuChoice::ChannelNotifications => "Notification settings",
        MenuChoice::MuteServer => "Mute server",
        MenuChoice::UnmuteServer => "Unmute server",
        MenuChoice::ServerNotifications => "Notification settings",
        MenuChoice::ShowAllChannels => "Show all channels",
        MenuChoice::HideMuted => "Hide muted channels",
        MenuChoice::Invite => "Invite to channel",
        MenuChoice::PinChannel => "Pin channel to top",
        MenuChoice::UnpinChannel => "Unpin channel",
        MenuChoice::AddToChannelList => "Add to channel list",
        MenuChoice::RemoveFromChannelList => "Remove from channel list",
        MenuChoice::UnfollowCategory => "Unfollow channel category",
        MenuChoice::CreateChannel => "Create text channel",
        MenuChoice::CreateCategory => "Create category",
        MenuChoice::EditChannel => "Edit channel",
        MenuChoice::DuplicateChannel => "Duplicate channel",
        MenuChoice::DeleteChannel => "Delete channel",
        MenuChoice::LeaveServer => "Leave server",
        MenuChoice::JoinThread => "Join thread",
        MenuChoice::LeaveThread => "Leave thread",
        MenuChoice::EditServerProfile => "Edit server profile",
        MenuChoice::ServerSettings => "Server settings",
    }
}
