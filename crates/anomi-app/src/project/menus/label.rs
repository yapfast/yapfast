//! What a menu row says for each of the core's choices, its icon, its group
//! and its id: `label`, `discord_rank`, `choice_id` and the row's looks in
//! `src/ui/menus.rs`.

use yapfast_core::application::MenuChoice;
use yapfast_lang::tr;

use anomi_vm::icon::Icon;

/// What a row says for a choice, and its icon.
pub(super) fn label(choice: MenuChoice, in_call: bool) -> (&'static str, Icon) {
    use MenuChoice as C;
    match choice {
        C::MarkRead => (tr!("Mark as read"), Icon::Check),
        C::JoinVoice => (tr!("Join voice"), Icon::Phone),
        C::OpenChat => (tr!("Open chat"), Icon::MessageSquareText),
        C::Profile => (tr!("Profile"), Icon::User),
        C::Message => (
            yapfast_lang::text_in("Menu", "Message"),
            Icon::MessageCircle,
        ),
        C::AddFriend => (tr!("Add friend"), Icon::UserPlus),
        C::AcceptFriend => (tr!("Accept friend request"), Icon::UserPlus),
        C::CancelFriendRequest => (tr!("Cancel friend request"), Icon::X),
        C::Pin => (tr!("Pin"), Icon::Pin),
        C::Unpin => (tr!("Unpin"), Icon::PinOff),
        C::Call if in_call => (tr!("Join call"), Icon::Phone),
        C::Call => (tr!("Start call"), Icon::Phone),
        C::AddNote => (tr!("Add note"), Icon::StickyNote),
        C::AddNickname => (tr!("Add friend nickname"), Icon::UserPen),
        C::EditNickname => (tr!("Change friend nickname"), Icon::UserPen),
        C::CloseDm => (tr!("Close DM"), Icon::X),
        C::LeaveGroup => (tr!("Leave group"), Icon::LogOut),
        C::InviteToServer => (tr!("Invite to server"), Icon::UserPlus),
        C::Apps => (tr!("Apps"), Icon::Settings),
        C::Unfriend => (tr!("Remove friend"), Icon::UserMinus),
        C::Ignore => (tr!("Ignore"), Icon::EyeOff),
        C::Unignore => (tr!("Stop ignoring"), Icon::EyeOff),
        C::Block => (tr!("Block"), Icon::Ban),
        C::Unblock => (tr!("Unblock"), Icon::Ban),
        C::Mute => (tr!("Menu" => "Mute"), Icon::BellOff),
        C::Unmute => (tr!("Menu" => "Unmute"), Icon::Bell),
        C::CopyLink => (tr!("Copy link"), Icon::Copy),
        C::CopyId => (tr!("Copy ID"), Icon::IdFilled),
        C::Mention => (tr!("Mention"), Icon::AtSign),
        C::ServerNickname => (tr!("Change nickname"), Icon::Pencil),
        C::Roles => (tr!("Roles"), Icon::Shield),
        C::RemoveTimeout => (tr!("Remove timeout"), Icon::Clock),
        C::Timeout => (tr!("Time out"), Icon::Clock),
        C::Kick => (tr!("Kick"), Icon::UserX),
        C::Ban => (tr!("Ban"), Icon::Gavel),
        C::TransferOwnership => (tr!("Transfer Ownership"), Icon::Crown),
        C::LocalMute => (tr!("Menu" => "Mute"), Icon::MicOff),
        C::SelfMute => (yapfast_lang::text_in("Microphone", "Mute"), Icon::MicOff),
        C::SoundboardMute => (tr!("Mute Soundboard"), Icon::Soundboard),
        C::SelfDeafen => (tr!("Deafen"), Icon::HeadphoneOff),
        C::MoveTo => (tr!("Move to"), Icon::Forward),
        C::ServerMute => (tr!("Server mute"), Icon::MicOff),
        C::ServerDeafen => (tr!("Server deafen"), Icon::HeadphoneOff),
        C::Disconnect => (tr!("Menu" => "Disconnect"), Icon::PhoneOff),
        // `Notify::dress` names these.
        C::ChannelMute => (tr!("Menu" => "Mute"), Icon::BellOff),
        C::ChannelNotifications => (tr!("Notification settings"), Icon::Bell),
        C::MuteServer => (tr!("Mute server"), Icon::BellOff),
        C::UnmuteServer => (tr!("Unmute server"), Icon::Bell),
        C::ServerNotifications => (tr!("Notification settings"), Icon::Bell),
        C::ShowAllChannels => (tr!("Show all channels"), Icon::Eye),
        C::HideMuted => (tr!("Hide muted channels"), Icon::EyeOff),
        // A server's says so in `super::rows`.
        C::Invite => (tr!("Invite to channel"), Icon::UserPlus),
        C::PinChannel => (tr!("Pin channel to top"), Icon::Pin),
        C::UnpinChannel => (tr!("Unpin channel"), Icon::PinOff),
        C::AddToChannelList => (tr!("Add to channel list"), Icon::Eye),
        C::RemoveFromChannelList => (tr!("Remove from channel list"), Icon::EyeOff),
        C::UnfollowCategory => (tr!("Unfollow channel category"), Icon::EyeOff),
        // A server's and a voice channel's say so in `super::rows`.
        C::CreateChannel => (tr!("Create text channel"), Icon::CirclePlus),
        C::CreateCategory => (tr!("Create category"), Icon::FolderPlus),
        // A category's say so in `super::rows`.
        C::EditChannel => (tr!("Edit channel"), Icon::Pencil),
        C::DuplicateChannel => (tr!("Duplicate channel"), Icon::CopyPlus),
        C::DeleteChannel => (tr!("Delete channel"), Icon::Trash),
        C::LeaveServer => (tr!("Leave server"), Icon::LogOut),
        C::JoinThread => (tr!("Join thread"), Icon::Thread),
        C::LeaveThread => (tr!("Leave thread"), Icon::LogOut),
        C::EditServerProfile => (tr!("Edit server profile"), Icon::Pencil),
        C::ServerSettings => (tr!("Server settings"), Icon::Settings),
    }
}

/// The group a choice starts, under a line where the group changes: what
/// it does, then how this account stands with someone, then
/// notifications, copying, roles, moderation; a channel's management, then
/// deleting it.
pub(super) fn group(choice: MenuChoice) -> u8 {
    use MenuChoice as C;
    match choice {
        C::InviteToServer
        | C::AddFriend
        | C::AcceptFriend
        | C::CancelFriendRequest
        | C::Unfriend
        | C::Ignore
        | C::Unignore
        | C::Block
        | C::Unblock => 1,
        C::Mute
        | C::Unmute
        | C::ChannelMute
        | C::ChannelNotifications
        | C::MuteServer
        | C::UnmuteServer
        | C::ServerNotifications
        | C::ShowAllChannels
        | C::HideMuted => 2,
        C::CopyLink
        | C::CopyId
        | C::JoinThread
        | C::LeaveThread
        | C::Invite
        | C::PinChannel
        | C::UnpinChannel
        | C::AddToChannelList
        | C::RemoveFromChannelList
        | C::UnfollowCategory => 3,
        C::ServerNickname | C::Roles | C::MoveTo => 4,
        C::Apps => 10,
        C::LocalMute | C::SoundboardMute | C::SelfMute | C::SelfDeafen => 8,
        C::ServerMute
        | C::ServerDeafen
        | C::Disconnect
        | C::Timeout
        | C::RemoveTimeout
        | C::Kick
        | C::Ban => 5,
        // Its own group, as in Discord's.
        C::TransferOwnership => 9,
        C::ServerSettings
        | C::EditServerProfile
        | C::CreateChannel
        | C::CreateCategory
        | C::EditChannel
        | C::DuplicateChannel => 6,
        C::DeleteChannel | C::LeaveServer | C::LeaveGroup => 7,
        _ => 0,
    }
}

/// A destructive choice, drawn in red.
pub(super) fn danger(choice: MenuChoice) -> bool {
    use MenuChoice as C;
    matches!(
        choice,
        C::Unfriend
            | C::Block
            | C::Timeout
            | C::Kick
            | C::Ban
            | C::ServerMute
            | C::ServerDeafen
            | C::Disconnect
            | C::DeleteChannel
            | C::LeaveServer
            | C::LeaveGroup
            | C::TransferOwnership
    )
}

/// A choice that turns something on or off, the menu staying open.
pub(super) fn checkbox(choice: MenuChoice) -> bool {
    use MenuChoice as C;
    matches!(
        choice,
        C::ShowAllChannels
            | C::HideMuted
            | C::LocalMute
            | C::SoundboardMute
            | C::SelfMute
            | C::SelfDeafen
            | C::ServerMute
            | C::ServerDeafen
    )
}

/// Where a choice goes in Discord's member menu: Profile, Mention, Message,
/// Start call and Add note; then Invite to server and how this account
/// stands with them; then the server's nickname and roles; then what the
/// call and moderation offer; Copy ID last. What a DM adds as a
/// conversation comes before them all.
fn discord_rank(choice: MenuChoice) -> u8 {
    use MenuChoice as C;
    match choice {
        C::MarkRead | C::JoinVoice | C::OpenChat => 0,
        C::Profile => 1,
        C::Mention => 2,
        C::Message => 3,
        C::Call => 4,
        C::AddNote => 5,
        C::AddNickname | C::EditNickname => 6,
        C::EditServerProfile => 7,
        C::Apps => 10,
        C::InviteToServer => 11,
        C::AddFriend | C::AcceptFriend | C::CancelFriendRequest | C::Unfriend => 12,
        C::Ignore | C::Unignore => 13,
        C::Block | C::Unblock => 14,
        C::ServerNickname => 20,
        C::Roles => 21,
        C::CopyId => 100,
        C::CopyLink => 99,
        _ => 50,
    }
}

/// Puts a person's choices in Discord's order.
pub(super) fn discord_order(choices: &mut [MenuChoice]) {
    choices.sort_by_key(|choice| discord_rank(*choice));
}

/// Every choice, for [`choice_of`].
const CHOICES: [MenuChoice; 66] = {
    use MenuChoice as C;
    [
        C::MarkRead,
        C::JoinVoice,
        C::OpenChat,
        C::Profile,
        C::Message,
        C::AddFriend,
        C::AcceptFriend,
        C::CancelFriendRequest,
        C::Pin,
        C::Unpin,
        C::Call,
        C::AddNote,
        C::AddNickname,
        C::EditNickname,
        C::CloseDm,
        C::LeaveGroup,
        C::InviteToServer,
        C::Apps,
        C::Unfriend,
        C::Ignore,
        C::Unignore,
        C::Block,
        C::Unblock,
        C::Mute,
        C::Unmute,
        C::CopyLink,
        C::CopyId,
        C::Mention,
        C::ServerNickname,
        C::Roles,
        C::Timeout,
        C::RemoveTimeout,
        C::Kick,
        C::Ban,
        C::TransferOwnership,
        C::ChannelMute,
        C::ChannelNotifications,
        C::MuteServer,
        C::UnmuteServer,
        C::ServerNotifications,
        C::HideMuted,
        C::Invite,
        C::PinChannel,
        C::UnpinChannel,
        C::AddToChannelList,
        C::RemoveFromChannelList,
        C::UnfollowCategory,
        C::ShowAllChannels,
        C::CreateChannel,
        C::CreateCategory,
        C::EditChannel,
        C::DuplicateChannel,
        C::DeleteChannel,
        C::LeaveServer,
        C::JoinThread,
        C::LeaveThread,
        C::EditServerProfile,
        C::ServerSettings,
        C::LocalMute,
        C::SoundboardMute,
        C::SelfMute,
        C::SelfDeafen,
        C::MoveTo,
        C::ServerMute,
        C::ServerDeafen,
        C::Disconnect,
    ]
};

/// The choice a row's id names.
pub(crate) fn choice_of(id: &str) -> Option<MenuChoice> {
    CHOICES.into_iter().find(|&choice| choice_id(choice) == id)
}

/// A choice's id, which its row's [`anomi_vm::menu::Choice`] carries.
pub(crate) fn choice_id(choice: MenuChoice) -> &'static str {
    use MenuChoice as C;
    match choice {
        C::MarkRead => "mark-read",
        C::JoinVoice => "join-voice",
        C::OpenChat => "open-chat",
        C::Profile => "profile",
        C::Message => "message",
        C::AddFriend => "add-friend",
        C::AcceptFriend => "accept-friend",
        C::CancelFriendRequest => "cancel-friend-request",
        C::Pin => "pin",
        C::Unpin => "unpin",
        C::Call => "call",
        C::AddNote => "add-note",
        C::AddNickname => "add-nickname",
        C::EditNickname => "edit-nickname",
        C::CloseDm => "close-dm",
        C::LeaveGroup => "leave-group",
        C::InviteToServer => "invite-to-server",
        C::Apps => "apps",
        C::Unfriend => "unfriend",
        C::Ignore => "ignore",
        C::Unignore => "unignore",
        C::Block => "block",
        C::Unblock => "unblock",
        C::Mute => "mute",
        C::Unmute => "unmute",
        C::CopyLink => "copy-link",
        C::CopyId => "copy-id",
        C::Mention => "mention",
        C::ServerNickname => "server-nickname",
        C::Roles => "roles",
        C::Timeout => "timeout",
        C::RemoveTimeout => "remove-timeout",
        C::Kick => "kick",
        C::Ban => "ban",
        C::TransferOwnership => "transfer-ownership",
        C::ChannelMute => "channel-mute",
        C::ChannelNotifications => "channel-notifications",
        C::MuteServer => "mute-server",
        C::UnmuteServer => "unmute-server",
        C::ServerNotifications => "server-notifications",
        C::HideMuted => "hide-muted",
        C::Invite => "invite",
        C::PinChannel => "pin-channel",
        C::UnpinChannel => "unpin-channel",
        C::AddToChannelList => "add-to-channel-list",
        C::RemoveFromChannelList => "remove-from-channel-list",
        C::UnfollowCategory => "unfollow-category",
        C::ShowAllChannels => "show-all-channels",
        C::CreateChannel => "create-channel",
        C::CreateCategory => "create-category",
        C::EditChannel => "edit-channel",
        C::DuplicateChannel => "duplicate-channel",
        C::DeleteChannel => "delete-channel",
        C::LeaveServer => "leave-server",
        C::JoinThread => "join-thread",
        C::LeaveThread => "leave-thread",
        C::EditServerProfile => "edit-server-profile",
        C::ServerSettings => "server-settings",
        C::LocalMute => "local-mute",
        C::SoundboardMute => "soundboard-mute",
        C::SelfMute => "self-mute",
        C::SelfDeafen => "self-deafen",
        C::MoveTo => "move-to",
        C::ServerMute => "server-mute",
        C::ServerDeafen => "server-deafen",
        C::Disconnect => "disconnect",
    }
}
