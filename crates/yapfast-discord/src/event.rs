//! The gateway's dispatched events by name, parsed once as they arrive so
//! the rest of the crate matches on [`Event`] rather than on strings.

/// Declares [`Event`] from each event's variant and Discord's name for it.
macro_rules! events {
    ($($(#[$doc:meta])* $variant:ident = $name:literal,)*) => {
        /// A dispatched event's name: one Yapfast acts on, or another.
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum Event {
            $($(#[$doc])* $variant,)*
            /// One Yapfast doesn't act on, by Discord's name.
            Other(String),
        }

        impl Event {
            /// The event Discord names `name`.
            pub fn parse(name: &str) -> Self {
                match name {
                    $($name => Self::$variant,)*
                    other => Self::Other(other.to_owned()),
                }
            }

            /// Discord's name for the event.
            pub fn name(&self) -> &str {
                match self {
                    $(Self::$variant => $name,)*
                    Self::Other(name) => name,
                }
            }
        }
    };
}

events! {
    Ready = "READY",
    Resumed = "RESUMED",
    /// What READY left out to be smaller: DMs, presences, voice states.
    ReadySupplemental = "READY_SUPPLEMENTAL",
    MessageCreate = "MESSAGE_CREATE",
    MessageUpdate = "MESSAGE_UPDATE",
    MessageDelete = "MESSAGE_DELETE",
    MessageDeleteBulk = "MESSAGE_DELETE_BULK",
    /// This account read a channel, here or on another device.
    MessageAck = "MESSAGE_ACK",
    MessageReactionAdd = "MESSAGE_REACTION_ADD",
    /// Reactions several people added within moments, in one event: the
    /// web client's capabilities, which Yapfast sends, ask for these in
    /// place of a MESSAGE_REACTION_ADD each.
    MessageReactionAddMany = "MESSAGE_REACTION_ADD_MANY",
    MessageReactionRemove = "MESSAGE_REACTION_REMOVE",
    MessageReactionRemoveAll = "MESSAGE_REACTION_REMOVE_ALL",
    MessageReactionRemoveEmoji = "MESSAGE_REACTION_REMOVE_EMOJI",
    MessagePollVoteAdd = "MESSAGE_POLL_VOTE_ADD",
    /// Votes several people cast within moments, as with reactions.
    MessagePollVoteAddMany = "MESSAGE_POLL_VOTE_ADD_MANY",
    MessagePollVoteRemove = "MESSAGE_POLL_VOTE_REMOVE",
    TypingStart = "TYPING_START",
    GuildCreate = "GUILD_CREATE",
    GuildUpdate = "GUILD_UPDATE",
    GuildDelete = "GUILD_DELETE",
    GuildMemberUpdate = "GUILD_MEMBER_UPDATE",
    /// Someone left a server, or was kicked or banned from it.
    GuildMemberRemove = "GUILD_MEMBER_REMOVE",
    /// Members Yapfast asked for, by id or by a search.
    GuildMembersChunk = "GUILD_MEMBERS_CHUNK",
    /// Discord refused a gateway command for being sent too often.
    RateLimited = "RATE_LIMITED",
    /// The member list of a channel Yapfast subscribed to.
    GuildMemberListUpdate = "GUILD_MEMBER_LIST_UPDATE",
    /// Someone was banned from a server, or unbanned.
    GuildBanAdd = "GUILD_BAN_ADD",
    GuildBanRemove = "GUILD_BAN_REMOVE",
    GuildRoleCreate = "GUILD_ROLE_CREATE",
    GuildRoleUpdate = "GUILD_ROLE_UPDATE",
    GuildRoleDelete = "GUILD_ROLE_DELETE",
    GuildEmojisUpdate = "GUILD_EMOJIS_UPDATE",
    GuildStickersUpdate = "GUILD_STICKERS_UPDATE",
    ChannelCreate = "CHANNEL_CREATE",
    ChannelUpdate = "CHANNEL_UPDATE",
    ChannelDelete = "CHANNEL_DELETE",
    /// Threads: one made, or that this account joined or was added to.
    ThreadCreate = "THREAD_CREATE",
    ThreadUpdate = "THREAD_UPDATE",
    ThreadDelete = "THREAD_DELETE",
    /// A channel's active threads, when this account comes to see it.
    ThreadListSync = "THREAD_LIST_SYNC",
    /// This account's membership of a thread changed: its settings there.
    ThreadMemberUpdate = "THREAD_MEMBER_UPDATE",
    /// Someone joined or left a thread, this account among them.
    ThreadMembersUpdate = "THREAD_MEMBERS_UPDATE",
    ChannelRecipientAdd = "CHANNEL_RECIPIENT_ADD",
    ChannelRecipientRemove = "CHANNEL_RECIPIENT_REMOVE",
    VoiceStateUpdate = "VOICE_STATE_UPDATE",
    VoiceServerUpdate = "VOICE_SERVER_UPDATE",
    /// Calls in DMs and group DMs.
    CallCreate = "CALL_CREATE",
    CallUpdate = "CALL_UPDATE",
    CallDelete = "CALL_DELETE",
    /// Go Live streams this account shares or watches.
    StreamCreate = "STREAM_CREATE",
    StreamUpdate = "STREAM_UPDATE",
    StreamDelete = "STREAM_DELETE",
    StreamServerUpdate = "STREAM_SERVER_UPDATE",
    /// Someone sent a soundboard sound or an emoji into this account's call.
    VoiceChannelEffectSend = "VOICE_CHANNEL_EFFECT_SEND",
    /// A server's soundboard sounds, asked for with op 31.
    SoundboardSounds = "SOUNDBOARD_SOUNDS",
    GuildSoundboardSoundCreate = "GUILD_SOUNDBOARD_SOUND_CREATE",
    GuildSoundboardSoundUpdate = "GUILD_SOUNDBOARD_SOUND_UPDATE",
    GuildSoundboardSoundDelete = "GUILD_SOUNDBOARD_SOUND_DELETE",
    GuildSoundboardSoundsUpdate = "GUILD_SOUNDBOARD_SOUNDS_UPDATE",
    RelationshipAdd = "RELATIONSHIP_ADD",
    RelationshipUpdate = "RELATIONSHIP_UPDATE",
    RelationshipRemove = "RELATIONSHIP_REMOVE",
    PresenceUpdate = "PRESENCE_UPDATE",
    /// Large servers' voice states, channels and members until subscribed.
    PassiveUpdateV1 = "PASSIVE_UPDATE_V1",
    PassiveUpdateV2 = "PASSIVE_UPDATE_V2",
    UserUpdate = "USER_UPDATE",
    /// The account's settings, in protobuf.
    UserSettingsProtoUpdate = "USER_SETTINGS_PROTO_UPDATE",
    /// Notification settings, per server or for DMs.
    UserGuildSettingsUpdate = "USER_GUILD_SETTINGS_UPDATE",
}

impl Event {
    /// One of the events about a Go Live stream.
    pub fn is_stream(&self) -> bool {
        matches!(
            self,
            Self::StreamCreate | Self::StreamUpdate | Self::StreamDelete | Self::StreamServerUpdate
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_event_yapfast_handles() {
        let handled = [
            ("READY", Event::Ready),
            ("RESUMED", Event::Resumed),
            ("READY_SUPPLEMENTAL", Event::ReadySupplemental),
            ("MESSAGE_CREATE", Event::MessageCreate),
            ("MESSAGE_UPDATE", Event::MessageUpdate),
            ("MESSAGE_DELETE", Event::MessageDelete),
            ("MESSAGE_DELETE_BULK", Event::MessageDeleteBulk),
            ("MESSAGE_ACK", Event::MessageAck),
            ("MESSAGE_REACTION_ADD", Event::MessageReactionAdd),
            ("MESSAGE_REACTION_ADD_MANY", Event::MessageReactionAddMany),
            ("MESSAGE_REACTION_REMOVE", Event::MessageReactionRemove),
            (
                "MESSAGE_REACTION_REMOVE_ALL",
                Event::MessageReactionRemoveAll,
            ),
            (
                "MESSAGE_REACTION_REMOVE_EMOJI",
                Event::MessageReactionRemoveEmoji,
            ),
            ("MESSAGE_POLL_VOTE_ADD", Event::MessagePollVoteAdd),
            ("MESSAGE_POLL_VOTE_ADD_MANY", Event::MessagePollVoteAddMany),
            ("MESSAGE_POLL_VOTE_REMOVE", Event::MessagePollVoteRemove),
            ("TYPING_START", Event::TypingStart),
            ("GUILD_CREATE", Event::GuildCreate),
            ("GUILD_UPDATE", Event::GuildUpdate),
            ("GUILD_DELETE", Event::GuildDelete),
            ("GUILD_MEMBER_UPDATE", Event::GuildMemberUpdate),
            ("GUILD_MEMBER_REMOVE", Event::GuildMemberRemove),
            ("GUILD_MEMBERS_CHUNK", Event::GuildMembersChunk),
            ("GUILD_MEMBER_LIST_UPDATE", Event::GuildMemberListUpdate),
            ("GUILD_BAN_ADD", Event::GuildBanAdd),
            ("GUILD_BAN_REMOVE", Event::GuildBanRemove),
            ("GUILD_ROLE_CREATE", Event::GuildRoleCreate),
            ("GUILD_ROLE_UPDATE", Event::GuildRoleUpdate),
            ("GUILD_ROLE_DELETE", Event::GuildRoleDelete),
            ("GUILD_EMOJIS_UPDATE", Event::GuildEmojisUpdate),
            ("GUILD_STICKERS_UPDATE", Event::GuildStickersUpdate),
            ("CHANNEL_CREATE", Event::ChannelCreate),
            ("CHANNEL_UPDATE", Event::ChannelUpdate),
            ("CHANNEL_DELETE", Event::ChannelDelete),
            ("THREAD_CREATE", Event::ThreadCreate),
            ("THREAD_UPDATE", Event::ThreadUpdate),
            ("THREAD_DELETE", Event::ThreadDelete),
            ("THREAD_LIST_SYNC", Event::ThreadListSync),
            ("THREAD_MEMBER_UPDATE", Event::ThreadMemberUpdate),
            ("THREAD_MEMBERS_UPDATE", Event::ThreadMembersUpdate),
            ("CHANNEL_RECIPIENT_ADD", Event::ChannelRecipientAdd),
            ("CHANNEL_RECIPIENT_REMOVE", Event::ChannelRecipientRemove),
            ("VOICE_STATE_UPDATE", Event::VoiceStateUpdate),
            ("VOICE_SERVER_UPDATE", Event::VoiceServerUpdate),
            ("CALL_CREATE", Event::CallCreate),
            ("CALL_UPDATE", Event::CallUpdate),
            ("CALL_DELETE", Event::CallDelete),
            ("STREAM_CREATE", Event::StreamCreate),
            ("STREAM_UPDATE", Event::StreamUpdate),
            ("STREAM_DELETE", Event::StreamDelete),
            ("STREAM_SERVER_UPDATE", Event::StreamServerUpdate),
            ("VOICE_CHANNEL_EFFECT_SEND", Event::VoiceChannelEffectSend),
            ("SOUNDBOARD_SOUNDS", Event::SoundboardSounds),
            (
                "GUILD_SOUNDBOARD_SOUND_CREATE",
                Event::GuildSoundboardSoundCreate,
            ),
            (
                "GUILD_SOUNDBOARD_SOUND_UPDATE",
                Event::GuildSoundboardSoundUpdate,
            ),
            (
                "GUILD_SOUNDBOARD_SOUND_DELETE",
                Event::GuildSoundboardSoundDelete,
            ),
            (
                "GUILD_SOUNDBOARD_SOUNDS_UPDATE",
                Event::GuildSoundboardSoundsUpdate,
            ),
            ("RELATIONSHIP_ADD", Event::RelationshipAdd),
            ("RELATIONSHIP_UPDATE", Event::RelationshipUpdate),
            ("RELATIONSHIP_REMOVE", Event::RelationshipRemove),
            ("PRESENCE_UPDATE", Event::PresenceUpdate),
            ("PASSIVE_UPDATE_V1", Event::PassiveUpdateV1),
            ("PASSIVE_UPDATE_V2", Event::PassiveUpdateV2),
            ("USER_UPDATE", Event::UserUpdate),
            ("USER_SETTINGS_PROTO_UPDATE", Event::UserSettingsProtoUpdate),
            ("USER_GUILD_SETTINGS_UPDATE", Event::UserGuildSettingsUpdate),
        ];
        for (name, event) in handled {
            assert_eq!(Event::parse(name), event, "{name}");
            assert_eq!(event.name(), name);
        }
    }

    #[test]
    fn keeps_other_events_by_name() {
        let other = Event::parse("GUILD_SCHEDULED_EVENT_CREATE");
        assert_eq!(other, Event::Other("GUILD_SCHEDULED_EVENT_CREATE".into()));
        assert_eq!(other.name(), "GUILD_SCHEDULED_EVENT_CREATE");
        assert_eq!(
            Event::parse("message_create"),
            Event::Other("message_create".into()),
            "names match exactly"
        );
        assert_eq!(
            Event::parse(""),
            Event::Other(String::new()),
            "a dispatch without a name"
        );
    }

    #[test]
    fn knows_the_stream_events() {
        let streams = [
            "STREAM_CREATE",
            "STREAM_UPDATE",
            "STREAM_DELETE",
            "STREAM_SERVER_UPDATE",
        ];
        for name in streams {
            assert!(Event::parse(name).is_stream(), "{name}");
        }
        for name in ["VOICE_SERVER_UPDATE", "STREAM_WATCH", "MESSAGE_CREATE"] {
            assert!(!Event::parse(name).is_stream(), "{name}");
        }
    }
}
