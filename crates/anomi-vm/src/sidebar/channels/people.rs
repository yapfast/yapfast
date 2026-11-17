//! Someone in a voice channel, as the channel list shows them under it and
//! on its card: `PersonRow` (`ui/voice.slint:6`) as `show_sidebar` fills it
//! for a channel's people and the window's participants
//! (`src/ui/refresh.rs:1414`).

use crate::kind::Avatar;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoicePerson {
    /// Who they are in the call, which their stream is watched by.
    pub id: String,
    /// Their user id, which their profile, menu and a move go by; empty
    /// for someone without one, who opens nothing and doesn't drag.
    pub user: String,
    pub name: String,
    pub avatar: Avatar,
    pub muted: bool,
    pub deafened: bool,
    /// Muted or deafened by the server: their marks show in red.
    pub server_muted: bool,
    pub server_deafened: bool,
    /// Sharing their screen: the LIVE tag.
    pub live: bool,
    /// Their camera is on.
    pub video: bool,
    /// This account: its LIVE tag doesn't open its own stream.
    pub local: bool,
    /// This account in the call on another client: dimmed until pointed
    /// at, without its marks.
    pub elsewhere: bool,
    /// Speaking, in this account's call: ringed, and in bold.
    pub speaking: bool,
    /// Connected, in this account's call; drawn muted until then.
    pub connected: bool,
    /// They may be dragged onto another voice channel: Move to offers one.
    pub movable: bool,
}

impl VoicePerson {
    /// Their mute and deafen marks show: always in this account's call,
    /// elsewhere not for this account on another client.
    pub fn marked(&self, in_call: bool) -> bool {
        in_call || !self.elsewhere
    }
}
