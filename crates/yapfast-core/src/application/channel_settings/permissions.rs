//! Discord's permissions as a channel's or category's Permissions page
//! lists them (`generateChannelPermissionSpec`): its groups for each kind
//! of channel, in its order, with Discord's titles and descriptions for a
//! channel, which differ from a role's. Those only behind Discord's
//! experiments aren't listed.

use crate::chat::ChannelKind;

const GENERAL: &[u32] = &[10, 4, 28, 29];
const MEMBERSHIP: &[u32] = &[0];
const TEXT: &[u32] = &[
    11, 38, 35, 36, 14, 15, 6, 18, 37, 17, 13, 51, 52, 34, 16, 12, 46, 49,
];
const VOICE: &[u32] = &[20, 21, 9, 42, 45, 25, 8, 22, 23, 24, 48];
const VOICE_CHAT: &[u32] = &[11, 14, 15, 6, 18, 37, 17, 13, 52, 16, 12, 46, 49];
const EVENTS: &[u32] = &[44, 33];
const APPS: &[u32] = &[31, 39, 50];

/// The groups of permissions a channel of `kind` shows, as headings with
/// their permissions' bit numbers.
pub fn channel_groups(kind: ChannelKind) -> &'static [(&'static str, &'static [u32])] {
    match kind {
        ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum => &[
            ("General Channel Permissions", GENERAL),
            ("Membership Permissions", MEMBERSHIP),
            ("Text Channel Permissions", TEXT),
            ("Apps Permissions", APPS),
        ],
        ChannelKind::Voice | ChannelKind::Stage => &[
            ("General Channel Permissions", GENERAL),
            ("Membership Permissions", MEMBERSHIP),
            ("Voice Channel Permissions", VOICE),
            ("Voice Channel Chat Permissions", VOICE_CHAT),
            ("Events Permissions", EVENTS),
            ("Apps Permissions", APPS),
        ],
        ChannelKind::Category => &[
            ("General Category Permissions", GENERAL),
            ("Membership Permissions", MEMBERSHIP),
            ("Text Channel Permissions", TEXT),
            ("Voice Channel Permissions", VOICE),
            ("Apps Permissions", APPS),
            ("Events Permissions", EVENTS),
        ],
    }
}

/// A permission's title and description in a channel of `kind`, as
/// Discord words them there; `everyone` when it's @everyone's overwrite.
pub fn channel_permission(
    bit: u32,
    kind: ChannelKind,
    everyone: bool,
) -> (&'static str, &'static str) {
    use ChannelKind::{Category, Voice};
    let category = kind == Category;
    // Most read "these channels" for a category, "this channel" otherwise.
    let these = |one: &'static str, many: &'static str| if category { many } else { one };
    match bit {
        10 => (
            these("View Channel", "View Channels"),
            match (everyone, kind) {
                (true, Category) => {
                    "Allows members to view these channels by default. Disabling this for @everyone will make text channels private. Disabling this and the Connect permission for @everyone will make voice channels private."
                }
                (true, Voice) => {
                    "Allows members to view this channel by default. Disabling this and the Connect permission for @everyone will make this channel private."
                }
                (true, _) => {
                    "Allows members to view this channel by default. Disabling this for @everyone will make this channel private."
                }
                (false, _) => these(
                    "Allows members to view this channel by default.",
                    "Allows members to view these channels by default.",
                ),
            },
        ),
        4 => (
            these("Manage Channel", "Manage Channels"),
            match kind {
                Category => {
                    "Allows members to change these channels’ name, description, and text / voice settings. They can also delete these channels."
                }
                Voice => {
                    "Allows members to change this channel’s name, description, and voice settings. They can also delete the channel."
                }
                _ => {
                    "Allows members to change this channel’s name, description, and text settings. They can also delete the channel."
                }
            },
        ),
        28 => (
            "Manage Permissions",
            these(
                "Allows members to change this channel’s permissions.",
                "Allows members to change these channels’ permissions.",
            ),
        ),
        29 => (
            "Manage Webhooks",
            these(
                "Allows members to create, edit, or delete webhooks in this channel, which can post messages from other apps or sites.",
                "Allows members to create, edit, or delete webhooks in these channels, which can post messages from other apps or sites.",
            ),
        ),
        0 => (
            "Create Invite",
            match kind {
                Category => {
                    "Allows members to invite new people to this server via a direct invite link to these channels."
                }
                Voice => {
                    "Allows members to invite new people to this server via a direct invite link to this channel. The recipient will automatically join the voice channel if they have permission to connect."
                }
                _ => {
                    "Allows members to invite new people to this server via a direct invite link to this channel."
                }
            },
        ),
        11 => (
            these("Send Messages", "Send Messages and Create Posts"),
            these(
                "Allows members to send messages in this channel.",
                "Allow members to send messages and create posts in these channels.",
            ),
        ),
        14 => (
            "Embed Links",
            these(
                "Allows links that members share to show embedded content in this channel.",
                "Allows links that members share to show embedded content in these channels.",
            ),
        ),
        15 => (
            "Attach Files",
            these(
                "Allows members to upload files or media in this channel.",
                "Allows members to upload files or media in these channels.",
            ),
        ),
        6 => (
            "Add Reactions",
            match kind {
                Category => {
                    "Allows members to add new emoji reactions to a message in these channels. If this permission is disabled, members can still react using any existing reactions on a message."
                }
                Voice => {
                    "Allows members to add new emoji reactions to a message in this channel and send reactions in voice channels. If this permission is disabled, members can still react using any existing reactions on a message."
                }
                _ => {
                    "Allows members to add new emoji reactions to a message in this channel. If this permission is disabled, members can still react using any existing reactions on a message."
                }
            },
        ),
        18 => (
            "Use External Emoji",
            these(
                "Allows members to use emoji from other servers in this channel, if they’re a Discord Nitro member.",
                "Allows members to use emoji from other servers in these channels, if they’re a Discord Nitro member.",
            ),
        ),
        37 => (
            "Use External Stickers",
            these(
                "Allows members to use stickers from other servers in this channel, if they’re a Discord Nitro member.",
                "Allows members to use stickers from other servers in these channels, if they’re a Discord Nitro member.",
            ),
        ),
        17 => (
            "Mention @everyone, @here, and All Roles",
            these(
                "Allows members to use @everyone or @here (only online members) in this channel. They can also @mention all roles in this channel, even if the role’s “Allow anyone to mention this role” permission is disabled.",
                "Allows members to use @everyone or @here (only online members) in these channels. They can also @mention all roles in these channels, even if the role’s “Allow anyone to mention this role” permission is disabled.",
            ),
        ),
        13 => (
            "Manage Messages",
            these(
                "Allows members to delete or remove embeds from messages by other members in this channel.",
                "Allows members to delete or remove embeds from messages by other members in these channels.",
            ),
        ),
        51 => (
            "Pin Messages",
            these(
                "Allows members to pin or unpin any message in this channel.",
                "Allows members to pin or unpin any message in these channels.",
            ),
        ),
        52 => (
            "Bypass Slowmode",
            these(
                "Allows members to send messages in this channel without being affected by slowmode.",
                "Allows members to send messages in these channels without being affected by slowmode.",
            ),
        ),
        16 => (
            "Read Message History",
            these(
                "Allows members to read previous messages sent in this channel. If this permission is disabled, members only see messages sent when they are online.",
                "Allows members to read previous messages sent in these channels. If this permission is disabled, members only see messages sent when they are online.",
            ),
        ),
        12 => (
            "Send Text-to-Speech Messages",
            these(
                "Allows members to send text-to-speech messages in this channel by starting a message with /tts. These messages can be heard by anyone focused on this channel.",
                "Allows members to send text-to-speech messages in these channels by starting a message with /tts. These messages can be heard by anyone focused on that channel.",
            ),
        ),
        31 => (
            "Use Application Commands",
            these(
                "Allows members to use commands from applications in this channel, including slash commands and context menu commands.",
                "Allows members to use commands from applications in these channels, including slash commands and context menu commands.",
            ),
        ),
        46 => (
            "Send Voice Messages",
            these(
                "Allows members to send voice messages in this channel. Requires the \"Attach Files\" permission as well.",
                "Allows members to send voice messages in these channels. Requires the \"Attach Files\" permission as well.",
            ),
        ),
        49 => (
            "Create Polls",
            these(
                "Allows members to create polls in this channel.",
                "Allows members to create polls in these channels.",
            ),
        ),
        20 => (
            "Connect",
            match (everyone, category) {
                (true, true) => {
                    "Allows members to join these voice channels and hear others. Disabling this and the View Channel permissions for @everyone will make these channels private."
                }
                (true, false) => {
                    "Allows members to join this voice channel and hear others. Disabling this and the View Channel permissions for @everyone will make this channel private."
                }
                (false, true) => "Allows members to join these voice channels and hear others.",
                (false, false) => "Allows members to join this voice channel and hear others.",
            },
        ),
        21 => (
            "Speak",
            these(
                "Allows members to talk in this voice channel. If this permission is disabled, members are default muted until somebody with the “Mute Members” permission un-mutes them.",
                "Allows members to talk in these voice channels. If this permission is disabled, members are default muted until somebody with the “Mute Members” permission un-mutes them.",
            ),
        ),
        9 => (
            "Video",
            these(
                "Allows members to share their video, screen share, or stream a game in this voice channel.",
                "Allows members to share their video, screen share, or stream a game in these voice channels.",
            ),
        ),
        39 => (
            "Use Activities",
            these(
                "Allows members to use Activities in this channel.",
                "Allows members to use Activities in these channels.",
            ),
        ),
        50 => (
            "Use External Apps",
            these(
                "Allows apps that members have added to their account to post messages in this channel. When disabled, the messages will be private.",
                "Allows apps that members have added to their account to post messages in these channels. When disabled, the messages will be private.",
            ),
        ),
        42 => (
            "Use Soundboard",
            these(
                "Allows members to send sounds from server soundboard in this voice channel.",
                "Allows members to send sounds from server soundboard in these voice channels.",
            ),
        ),
        45 => (
            "Use External Sounds",
            "Allows members to use sounds from other servers, if they're a Discord Nitro member.",
        ),
        25 => (
            "Use Voice Activity",
            these(
                "Allows members to speak in this voice channel by simply talking. If this permission is disabled, members are required to use Push-to-talk. Good for controlling background noise or noisy members.",
                "Allows members to speak in these voice channels by simply talking. If this permission is disabled, members are required to use Push-to-talk. Good for controlling background noise or noisy members.",
            ),
        ),
        8 => (
            "Priority Speaker",
            these(
                "Allows members to be more easily heard in this voice channel. When activated, the volume of others without this permission will be automatically lowered. Priority Speaker is activated by using the Push to Talk (Priority) keybind.",
                "Allows members to be more easily heard in these voice channels. When activated, the volume of others without this permission will be automatically lowered. Priority Speaker is activated by using the Push to Talk (Priority) keybind.",
            ),
        ),
        22 => (
            "Mute Members",
            these(
                "Allows members to mute other members in this voice channel for everyone.",
                "Allows members to mute other members in these voice channels for everyone.",
            ),
        ),
        23 => (
            "Deafen Members",
            these(
                "Allows members to deafen other members in this voice channel, which means they won’t be able to speak or hear others.",
                "Allows members to deafen other members in these voice channels, which means they won’t be able to speak or hear others.",
            ),
        ),
        24 => (
            "Move Members",
            these(
                "Allows members to disconnect other members from this channel. They will also be able to move somebody into other channels that they have this permission in.",
                "Allows members to disconnect other members from these channels. They will also be able to move somebody into other channels that they have this permission in.",
            ),
        ),
        34 => (
            these("Manage Threads", "Manage Threads and Posts"),
            these(
                "Allows members to rename, delete, close, and turn on slow mode for threads in this channel. They can also view private threads.",
                "Allows members to rename, delete, close, and turn on slow mode for threads and posts in these channels. They can also view private threads.",
            ),
        ),
        35 => (
            "Create Public Threads",
            these(
                "Allow members to create threads that everyone in this channel can view.",
                "Allow members to create threads that everyone in these channels can view.",
            ),
        ),
        36 => (
            "Create Private Threads",
            these(
                "Allow members to create invite-only threads in this channel.",
                "Allow members to create invite-only threads in these channels.",
            ),
        ),
        38 => (
            these(
                "Send Messages in Threads",
                "Send Messages in Threads and Posts",
            ),
            these(
                "Allow members to send messages in threads under this channel.",
                "Allow members to send messages in threads and posts under these channels.",
            ),
        ),
        33 => (
            "Manage Events",
            these(
                "Allows members to edit and cancel events in this channel.",
                "Allows members to edit and cancel events in these channels.",
            ),
        ),
        44 => (
            "Create Events",
            these(
                "Allows members to create events in this channel.",
                "Allows members to create events in these channels.",
            ),
        ),
        48 => (
            "Set Voice Channel Status",
            "Allows members to create and edit voice channel status.",
        ),
        _ => ("", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_lists_discords_permissions_each_with_its_words() {
        let count = |kind| -> usize {
            channel_groups(kind)
                .iter()
                .map(|(_, bits)| bits.len())
                .sum()
        };
        assert_eq!(
            (
                count(ChannelKind::Text),
                count(ChannelKind::Voice),
                count(ChannelKind::Category)
            ),
            (26, 34, 39)
        );
        for kind in [ChannelKind::Text, ChannelKind::Voice, ChannelKind::Category] {
            for (_, bits) in channel_groups(kind) {
                for &bit in *bits {
                    let (title, description) = channel_permission(bit, kind, false);
                    assert!(!title.is_empty() && !description.is_empty(), "{bit}");
                }
            }
        }
        assert_eq!(
            channel_permission(10, ChannelKind::Text, true).1,
            "Allows members to view this channel by default. Disabling this for @everyone will make this channel private."
        );
        assert_eq!(
            channel_permission(4, ChannelKind::Category, false).0,
            "Manage Channels"
        );
    }
}
