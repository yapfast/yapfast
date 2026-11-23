//! What a plugin is told about, and the filters that decide what reaches it.
//!
//! Events are shaped by network, with Discord first, so Matrix can follow
//! without a breaking change. The parent serializes one event
//! into an `Arc<[u8]>` and the helper hands the same bytes to each subscriber;
//! events nobody subscribed to are never serialized at all.

use serde::{Deserialize, Serialize};

use crate::limits;

/// Which network an event came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    Discord,
    Matrix,
}

/// The kinds of event a plugin may subscribe to.
///
/// The first group is sent as it happens. `typing`, `presence`, `speaking`,
/// `member_join`, `member_leave` and `composer_change` are opt-in and coalesced: a burst arrives as one call with several events,
/// at most every 250 ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    MessageCreate,
    MessageUpdate,
    MessageDelete,
    ReactionAdd,
    ReactionRemove,
    RoomSelected,
    VoiceState,
    AppFocus,
    AccountReady,
    /// Opt-in and coalesced.
    Typing,
    /// Opt-in and coalesced.
    Presence,
    /// Opt-in and coalesced.
    Speaking,
    /// Someone appeared in the open room's member list: opt-in and coalesced.
    MemberJoin,
    /// Someone left the open room's member list: opt-in and coalesced.
    MemberLeave,
    /// What the linked Spotify plays changed: a song, a pause, a seek.
    SpotifyState,
    /// How long this start of Yapfast took, once a session: `app.timing`.
    StartupTiming,
    /// Someone became a friend: `relationships.read`.
    RelationshipAdd,
    /// A friend, or a request, went: `relationships.read`.
    RelationshipRemove,
    /// A friend request arrived or was sent: `relationships.read`.
    RelationshipRequest,
    /// The account joined a server, or was added to one: `servers.read`.
    ServerJoin,
    /// A server left the account's list: it left, was kicked or banned, or the
    /// server was deleted. Not an outage, which keeps it listed: `servers.read`.
    ServerRemove,
    /// A group DM left the account's list: it left or was removed, or the
    /// group was deleted: `servers.read`.
    GroupDmRemove,
    /// The game Yapfast sees this account playing started: `activity.read`.
    GameStart,
    /// That game stopped: `activity.read`.
    GameStop,
    /// The text in a composer changed: `drafts.read`. Opt-in and coalesced.
    ComposerChange,
}

impl EventKind {
    /// Every kind a manifest may name after `event:`.
    pub const ALL: [EventKind; 25] = [
        EventKind::MessageCreate,
        EventKind::MessageUpdate,
        EventKind::MessageDelete,
        EventKind::ReactionAdd,
        EventKind::ReactionRemove,
        EventKind::RoomSelected,
        EventKind::VoiceState,
        EventKind::AppFocus,
        EventKind::AccountReady,
        EventKind::Typing,
        EventKind::Presence,
        EventKind::Speaking,
        EventKind::MemberJoin,
        EventKind::MemberLeave,
        EventKind::SpotifyState,
        EventKind::StartupTiming,
        EventKind::RelationshipAdd,
        EventKind::RelationshipRemove,
        EventKind::RelationshipRequest,
        EventKind::ServerJoin,
        EventKind::ServerRemove,
        EventKind::GroupDmRemove,
        EventKind::GameStart,
        EventKind::GameStop,
        EventKind::ComposerChange,
    ];

    /// The name written after `event:` in a manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::MessageCreate => "message_create",
            EventKind::MessageUpdate => "message_update",
            EventKind::MessageDelete => "message_delete",
            EventKind::ReactionAdd => "reaction_add",
            EventKind::ReactionRemove => "reaction_remove",
            EventKind::RoomSelected => "room_selected",
            EventKind::VoiceState => "voice_state",
            EventKind::AppFocus => "app_focus",
            EventKind::AccountReady => "account_ready",
            EventKind::Typing => "typing",
            EventKind::Presence => "presence",
            EventKind::Speaking => "speaking",
            EventKind::MemberJoin => "member_join",
            EventKind::MemberLeave => "member_leave",
            EventKind::SpotifyState => "spotify_state",
            EventKind::StartupTiming => "startup_timing",
            EventKind::RelationshipAdd => "relationship_add",
            EventKind::RelationshipRemove => "relationship_remove",
            EventKind::RelationshipRequest => "relationship_request",
            EventKind::ServerJoin => "server_join",
            EventKind::ServerRemove => "server_remove",
            EventKind::GroupDmRemove => "group_dm_remove",
            EventKind::GameStart => "game_start",
            EventKind::GameStop => "game_stop",
            EventKind::ComposerChange => "composer_change",
        }
    }

    /// Whether a burst of these is batched rather than sent one by one.
    pub fn is_coalesced(self) -> bool {
        matches!(
            self,
            EventKind::Typing
                | EventKind::Presence
                | EventKind::Speaking
                | EventKind::MemberJoin
                | EventKind::MemberLeave
                | EventKind::ComposerChange
        )
    }
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One event, with everything a filter reads kept at the top.
///
/// `payload` is JSON because the wire format is JSON in v1: the same bytes go
/// to every subscriber, so nothing is decoded or copied per plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub kind: EventKind,
    pub network: Network,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// The room it happened in, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
    /// The person it is about, when it is about someone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Whether the account did it itself.
    #[serde(default)]
    pub own: bool,
    pub payload: serde_json::Value,
}

impl EventEnvelope {
    /// The event as one batch entry: its bytes, with the long payload cut to
    /// [`limits::MAX_PAYLOAD_BYTES`] and flagged, so no event is dropped for
    /// being large.
    pub fn truncated(mut self) -> Self {
        cut_payload(&mut self.payload);
        self
    }
}

/// Cuts a payload whose JSON is over [`limits::MAX_PAYLOAD_BYTES`] to that many
/// bytes of its text, flagged with its whole size.
///
/// Measured by counting what the encoder writes, so the payload of every event
/// that fits, which is nearly all of them, is never copied into a string.
fn cut_payload(payload: &mut serde_json::Value) {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    if serde_json::to_writer(&mut count, payload).is_err() || count.0 <= limits::MAX_PAYLOAD_BYTES {
        return;
    }
    let text = payload.to_string();
    // On a character boundary: a byte offset that lands inside a multi-byte
    // character would panic, and a message in a script that isn't ASCII is
    // ordinary, not exceptional.
    let mut cut = limits::MAX_PAYLOAD_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    *payload = serde_json::json!({
        "truncated": true,
        "bytes": text.len(),
        "text": &text[..cut],
    });
}

/// One `plugin.publish`, as a plugin that subscribed to its topic receives it.
///
/// A topic event travels in the same `yf_event` batch as events, under
/// `topics`, but it is not one of [`EventKind`]: a plugin subscribes to a
/// topic by name from its own code, so the set of topics is the plugins' own
/// and not the ABI's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TopicEvent {
    /// The topic the publisher named.
    pub topic: String,
    /// The plugin that published it.
    pub from: crate::ids::PluginId,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// The publisher's own JSON, cut and flagged as an event's payload is.
    pub payload: serde_json::Value,
}

impl TopicEvent {
    /// The topic event with a payload over [`limits::MAX_PAYLOAD_BYTES`] cut and
    /// flagged, so a subscriber still sees that the message was long.
    pub fn truncated(mut self) -> Self {
        cut_payload(&mut self.payload);
        self
    }
}

/// What a subscription lets through, applied in the parent before the helper
/// is woken.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SubscriptionFilter {
    /// One room only, or every room.
    pub room: Option<String>,
    /// Leave the account's own messages out, as a plugin that only cares about
    /// what other people say does.
    pub exclude_own: bool,
}

impl SubscriptionFilter {
    /// Whether `event` passes this filter.
    pub fn allows(&self, event: &EventEnvelope) -> bool {
        if self.exclude_own && event.own {
            return false;
        }
        match (&self.room, &event.room) {
            (Some(wanted), Some(room)) => wanted == room,
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

/// A message as a plugin sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub id: String,
    pub room: String,
    pub author: PersonView,
    pub body: String,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    #[serde(default)]
    pub edited: bool,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub attachments: Vec<AttachmentView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// Whether this message mentions the account.
    #[serde(default)]
    pub mentions_me: bool,
    /// Among its conversation's pinned messages.
    #[serde(default)]
    pub pinned: bool,
    /// A Discord webhook sent it: `author` is the name and picture the
    /// webhook gave, not an account's, so `people.get` knows nobody by its id.
    #[serde(default)]
    pub webhook: bool,
    /// The stickers it carries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stickers: Vec<StickerView>,
}

/// A sticker a message carries: its Discord id, which a sticker read or a
/// menu row names, and its name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StickerView {
    pub id: String,
    pub name: String,
}

impl StickerView {
    /// A sticker's id, read from its picture's address
    /// (`…/stickers/<id>.png`).
    pub fn id_of(url: &str) -> Option<&str> {
        let tail = url.split("/stickers/").nth(1)?;
        let id = tail.split(['.', '?', '/']).next()?;
        (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then_some(id)
    }
}

/// A person, as every call and event names them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonView {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Whether this is the account's own.
    #[serde(default)]
    pub own: bool,
    #[serde(default)]
    pub bot: bool,
}

/// A file attached to a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentView {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    /// A `discord://` or `matrix://` reference, which `media.get` reads; never
    /// an authenticated URL, since a plugin gets no token.
    pub media: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// A room a plugin can be shown or told about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomView {
    pub id: String,
    pub name: String,
    pub network: Network,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default)]
    pub dm: bool,
    /// Age-restricted: Discord's NSFW channel.
    #[serde(default)]
    pub nsfw: bool,
    /// Who is in it now, for a voice channel or a DM's call, as the sidebar
    /// shows them without joining: no one speaks here, since only the call
    /// this account is in says who talks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voice: Vec<crate::call::VoicePerson>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: EventKind, room: Option<&str>, own: bool) -> EventEnvelope {
        EventEnvelope {
            kind,
            network: Network::Discord,
            at: 0,
            room: room.map(str::to_owned),
            author: None,
            own,
            payload: serde_json::Value::Null,
        }
    }

    #[test]
    fn a_filter_keeps_only_its_room_and_leaves_own_messages_behind() {
        let filter = SubscriptionFilter {
            room: Some("room-1".into()),
            exclude_own: true,
        };
        assert!(filter.allows(&event(EventKind::MessageCreate, Some("room-1"), false)));
        assert!(!filter.allows(&event(EventKind::MessageCreate, Some("room-1"), true)));
        assert!(!filter.allows(&event(EventKind::MessageCreate, Some("room-2"), false)));
        assert!(!filter.allows(&event(EventKind::MessageCreate, None, false)));
    }

    #[test]
    fn an_empty_filter_lets_everything_through() {
        let filter = SubscriptionFilter::default();
        assert!(filter.allows(&event(EventKind::MessageCreate, None, true)));
        assert!(filter.allows(&event(EventKind::AppFocus, None, false)));
    }

    #[test]
    fn a_large_payload_is_cut_and_flagged_rather_than_dropped() {
        let mut big = event(EventKind::MessageCreate, Some("room-1"), false);
        big.payload = serde_json::json!({ "body": "x".repeat(limits::MAX_PAYLOAD_BYTES * 2) });
        let cut = big.truncated();
        assert_eq!(cut.payload["truncated"], true);
        assert!(serde_json::to_string(&cut.payload).unwrap().len() < limits::MAX_PAYLOAD_BYTES * 2);
    }

    #[test]
    fn kind_names_are_the_manifest_s_names() {
        for kind in EventKind::ALL {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
        }
    }
}

#[cfg(test)]
mod sticker_tests {
    use super::StickerView;

    #[test]
    fn a_stickers_id_is_read_from_its_address() {
        assert_eq!(
            StickerView::id_of("https://media.discordapp.net/stickers/8.gif?size=160"),
            Some("8")
        );
        assert_eq!(
            StickerView::id_of("https://cdn.discordapp.com/stickers/9.json"),
            Some("9")
        );
        assert_eq!(StickerView::id_of("https://example.com/cat.png"), None);
    }
}
