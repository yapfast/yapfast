//! What the application knows, kept where the bridge answers a guest's reads
//! from.
//!
//! A host call is answered in the bridge's own task, which is not the thread
//! that holds the application's state, so the few facts a read needs are
//! published here as they change: the account's own name, the lists, the open
//! room's timeline and its members. Publishing is an `Arc` clone or a short
//! `String`, never a copy of a timeline, and it happens only when the
//! application's own counters say something changed — see
//! `Application::plugins_sync_world`. With no plugin enabled nothing is
//! published at all.
//!
//! The answers are built here rather than in `bridge`, so the bridge's match
//! stays one line per method; each is capped by
//! [`yapfast_plugin_api::frame::MAX_FRAME`], because one answer is one frame,
//! and a plugin asking for a thousand messages is told what it got rather than
//! cut off mid-frame.
//!
//! What a read may say is decided by what the application already knew for the
//! interface: the account, the rooms it lists, the room on screen and the
//! people in it. Nothing here asks a network for anything, and no answer
//! carries a token.

use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard};

use yapfast_plugin_api::call::{AccountView, AppInfo, MemberView};
use yapfast_plugin_api::event::{MessageView, PersonView, RoomView};
use yapfast_plugin_api::frame::MAX_FRAME;

use crate::chat::{ChatMember, ChatMessage, ChatRoom};

/// The room an id names, as the room list answers it.
///
/// One answer carries at most this much: the frame's cap minus the room the
/// answer's own object takes, so a list of long names still fits in one frame.
const ANSWER_BYTES: usize = MAX_FRAME - 4096;

/// The most rooms one `rooms.list` answers with, whatever it asks for.
pub const MAX_ROOMS: usize = 200;

/// The most messages one `messages.history` answers with.
pub const MAX_MESSAGES: usize = 100;

/// The account, as `account.me` answers it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Me {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub avatar: Option<String>,
    pub status: Option<String>,
    pub trusted: bool,
}

/// The open room's timeline, as the application last showed it.
struct Timeline {
    room: String,
    /// Shared with the application's own state, so publishing one costs a
    /// count and answers a page without a copy.
    items: Arc<Vec<Arc<ChatMessage>>>,
}

/// The open room's member list.
struct Members {
    room: String,
    people: Arc<Vec<ChatMember>>,
}

#[derive(Default)]
struct Held {
    me: Option<Me>,
    version: String,
    /// `light` or `dark`, once the interface said.
    theme: Option<&'static str>,
    /// The list generation the rooms were published at, as
    /// [`crate::application::AppState::list_changes`] counts it.
    lists: Option<u64>,
    /// The rooms as the sidebar lists them, translated once per change rather
    /// than per answer.
    rooms: Arc<Vec<RoomView>>,
    selected: Option<String>,
    timeline: Option<Timeline>,
    members: Option<Members>,
}

/// What the bridge answers a guest's reads from.
#[derive(Default)]
pub struct World {
    held: RwLock<Held>,
}

impl World {
    /// A world with the version this Yapfast is, which never changes.
    pub fn new(version: impl Into<String>) -> Self {
        World {
            held: RwLock::new(Held {
                version: version.into(),
                ..Held::default()
            }),
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, Held> {
        self.held.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// The account, or nothing while nobody is signed in.
    ///
    /// Compared before it is kept, because this runs on every event the
    /// application handles: an account that hasn't changed costs five string
    /// comparisons and no copy.
    pub fn publish_account(
        &self,
        id: Option<&str>,
        name: Option<&str>,
        username: Option<&str>,
        avatar: Option<&str>,
        status: Option<&str>,
        trusted: bool,
    ) {
        {
            let held = self.read();
            let same = match (&held.me, id) {
                (None, None) => true,
                (Some(me), Some(id)) => {
                    me.id == id
                        && me.name == name.unwrap_or_default()
                        && me.username.as_deref() == username
                        && me.avatar.as_deref() == avatar
                        && me.status.as_deref() == status
                        && me.trusted == trusted
                }
                _ => false,
            };
            if same {
                return;
            }
        }
        let me = id.map(|id| Me {
            id: id.to_owned(),
            name: name.unwrap_or_default().to_owned(),
            username: username.map(str::to_owned),
            avatar: avatar.map(str::to_owned),
            status: status.map(str::to_owned),
            trusted,
        });
        self.held.write().unwrap_or_else(PoisonError::into_inner).me = me;
    }

    /// Whether the interface's theme is dark, which `app.theme_mode` answers.
    pub fn publish_theme(&self, dark: bool) {
        self.held
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .theme = Some(if dark { "dark" } else { "light" });
    }

    /// The list generation the rooms were published at, so the application
    /// translates them only when they changed.
    pub fn lists(&self) -> Option<u64> {
        self.read().lists
    }

    /// The lists, at the generation `changes` counts. The application builds
    /// them only when [`World::lists`] says they moved.
    pub fn publish_lists(&self, changes: u64, rooms: Vec<RoomView>) {
        let mut held = self.held.write().unwrap_or_else(PoisonError::into_inner);
        held.rooms = Arc::new(rooms);
        held.lists = Some(changes);
    }

    /// The open room, by the id the sidebar gives it.
    pub fn publish_selected(&self, selected: Option<&str>) {
        {
            let held = self.read();
            if held.selected.as_deref() == selected {
                return;
            }
        }
        self.held
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .selected = selected.map(str::to_owned);
    }

    /// The open room's timeline, which is the only one the application keeps.
    pub fn publish_timeline(&self, room: &str, items: &Arc<Vec<Arc<ChatMessage>>>) {
        {
            let held = self.read();
            let same = held
                .timeline
                .as_ref()
                .is_some_and(|held| held.room == room && Arc::ptr_eq(&held.items, items));
            if same {
                return;
            }
        }
        let mut held = self.held.write().unwrap_or_else(PoisonError::into_inner);
        held.timeline = Some(Timeline {
            room: room.to_owned(),
            items: Arc::clone(items),
        });
    }

    /// The open room's members.
    pub fn publish_members(&self, room: &str, people: &Arc<Vec<ChatMember>>) {
        {
            let held = self.read();
            let same = held
                .members
                .as_ref()
                .is_some_and(|held| held.room == room && Arc::ptr_eq(&held.people, people));
            if same {
                return;
            }
        }
        let mut held = self.held.write().unwrap_or_else(PoisonError::into_inner);
        held.members = Some(Members {
            room: room.to_owned(),
            people: Arc::clone(people),
        });
    }

    /// `app.version`, `app.locale` and `app.theme_mode`.
    ///
    /// Yapfast has one language and one window, so all three name the same
    /// answer. The language is the system's: Yapfast reads `LC_ALL`, then
    /// `LANG`, and answers `en-GB` when neither says, which is what it shows.
    pub fn app(&self) -> AppInfo {
        let held = self.read();
        AppInfo {
            version: held.version.clone(),
            locale: system_locale(),
            theme_mode: held.theme.unwrap_or("dark").to_owned(),
        }
    }

    /// `account.me`.
    pub fn me(&self) -> Option<AccountView> {
        let held = self.read();
        held.me.as_ref().map(|me| AccountView {
            id: me.id.clone(),
            name: me.name.clone(),
            username: me.username.clone(),
            avatar: me.avatar.clone(),
            status: me.status.clone(),
            trusted: me.trusted,
        })
    }

    /// `rooms.selected`.
    pub fn selected(&self) -> Option<RoomView> {
        let held = self.read();
        let selected = held.selected.as_deref()?;
        held.rooms.iter().find(|room| room.id == selected).cloned()
    }

    /// `rooms.get`.
    pub fn room(&self, id: &str) -> Option<RoomView> {
        let held = self.read();
        held.rooms.iter().find(|room| room.id == id).cloned()
    }

    /// `rooms.list`, in the order the sidebar lists them, cut to `limit` and to
    /// one frame.
    pub fn rooms(&self, limit: Option<usize>) -> Vec<RoomView> {
        let held = self.read();
        let most = limit.unwrap_or(MAX_ROOMS).min(MAX_ROOMS);
        let mut out = Vec::new();
        let mut bytes = 2;
        for room in held.rooms.iter().take(most) {
            let size = serde_json::to_string(room).map_or(0, |json| json.len() + 1);
            if bytes + size > ANSWER_BYTES {
                break;
            }
            bytes += size;
            out.push(room.clone());
        }
        out
    }

    /// `messages.get`: one message of the open room.
    pub fn message(&self, room: &str, id: &str) -> Option<MessageView> {
        let held = self.read();
        let timeline = held.timeline.as_ref().filter(|held| held.room == room)?;
        timeline
            .items
            .iter()
            .find(|message| message.id == id)
            .map(|message| message_view(message, room))
    }

    /// `messages.history`: a page of the open room's timeline, oldest first, as
    /// the timeline orders it. `before` names the message to stop at, and the
    /// newest page is what the room has when it's absent.
    ///
    /// `None` when the room isn't the one on screen: the application keeps one
    /// timeline, and answering for another room would be inventing one.
    pub fn history(
        &self,
        room: &str,
        before: Option<&str>,
        limit: Option<usize>,
    ) -> Option<Vec<MessageView>> {
        let held = self.read();
        let timeline = held.timeline.as_ref().filter(|held| held.room == room)?;
        let end = match before {
            Some(before) => timeline
                .items
                .iter()
                .position(|message| message.id == before)
                .unwrap_or(timeline.items.len()),
            None => timeline.items.len(),
        };
        let most = limit.unwrap_or(MAX_MESSAGES).min(MAX_MESSAGES);
        let mut out: Vec<MessageView> = Vec::new();
        let mut bytes = 2;
        for message in timeline.items[..end].iter().rev().take(most) {
            let message = message_view(message, room);
            let size = serde_json::to_string(&message).map_or(0, |json| json.len() + 1);
            if bytes + size > ANSWER_BYTES {
                break;
            }
            bytes += size;
            out.push(message);
        }
        out.reverse();
        Some(out)
    }

    /// `people.get`: the account itself, one of the open room's members, or
    /// nobody.
    pub fn person(&self, id: &str) -> Option<PersonView> {
        let held = self.read();
        if let Some(me) = held.me.as_ref().filter(|me| me.id == id) {
            return Some(PersonView {
                id: me.id.clone(),
                name: me.name.clone(),
                display_name: me.username.clone(),
                own: true,
                bot: false,
            });
        }
        let members = held.members.as_ref()?;
        members
            .people
            .iter()
            .find(|member| member.id == id)
            .map(person)
    }

    /// `people.member`: one person as one server sees them.
    ///
    /// Only the conversation on screen has its members in memory, so a question
    /// about any other server is [`Members::NotOpen`]: the caller can say which
    /// one it would have to be.
    pub fn member(&self, server: &str, id: &str) -> MemberAnswer {
        let held = self.read();
        let Some(members) = held.members.as_ref() else {
            return MemberAnswer::NotOpen;
        };
        if room_server(&members.room).as_deref() != Some(server) {
            return MemberAnswer::NotOpen;
        }
        MemberAnswer::Open(
            members
                .people
                .iter()
                .find(|member| member.id == id)
                .map(|member| MemberView {
                    person: person(member),
                    roles: member.roles.iter().cloned().collect(),
                    presence: member
                        .presence
                        .map(|around| crate::plugins::presence::name(around).to_owned()),
                    colour: member.color,
                    activity: member.activity.clone(),
                    timed_out_until: member.timed_out_until,
                }),
        )
    }
}

/// What `people.member` found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemberAnswer {
    /// The server's members are the open conversation's, and this is who is
    /// there, when anyone with that id is.
    Open(Option<MemberView>),
    /// The members of another server aren't in memory: only the conversation on
    /// screen has its people loaded.
    NotOpen,
}

impl MemberAnswer {
    /// The member, or nothing, when the server's list is the open one.
    pub fn open(self) -> Option<Option<MemberView>> {
        match self {
            MemberAnswer::Open(found) => Some(found),
            MemberAnswer::NotOpen => None,
        }
    }
}

/// The server a room id belongs to, for Discord's channel ids; nothing for a
/// DM or a Matrix room, which no server holds.
fn room_server(room: &str) -> Option<String> {
    crate::discord::parse_room(room)
        .map(|(server, _)| server.to_owned())
        .filter(|server| server != crate::discord::DM_GUILD)
}

/// A room as a plugin sees it: what the sidebar shows of it, and the server it
/// belongs to, which is how a plugin tells a channel from a conversation.
pub fn room_view(room: &ChatRoom) -> RoomView {
    RoomView {
        id: room.id.clone(),
        name: room.name.clone(),
        network: match room.network {
            crate::network::Network::Discord => yapfast_plugin_api::event::Network::Discord,
            crate::network::Network::Matrix => yapfast_plugin_api::event::Network::Matrix,
        },
        server: room_server(&room.id),
        dm: room.direct,
    }
}

/// A member as a plugin sees them.
fn person(member: &ChatMember) -> PersonView {
    PersonView {
        id: member.id.clone(),
        name: member.name.clone(),
        display_name: member.username.clone(),
        own: false,
        bot: member.bot,
    }
}

/// A message of the open room as a plugin sees it.
///
/// The body is the text the timeline shows, and its files are left out the way
/// an event's are: a plugin gets no token, and `media.get` is where bytes come
/// from.
fn message_view(message: &ChatMessage, room: &str) -> MessageView {
    MessageView {
        id: message.id.clone(),
        room: room.to_owned(),
        author: PersonView {
            id: message.sender.clone(),
            name: message.author.clone(),
            display_name: None,
            own: message.own,
            bot: message.bot,
        },
        body: message.body.clone(),
        at: message.timestamp * 1000,
        edited: message.edited,
        deleted: message.deleted,
        attachments: Vec::new(),
        reply_to: message.reply.as_ref().and_then(|reply| reply.item.clone()),
        mentions_me: message.mentions_me,
    }
}

/// The person's language, from the environment: `LC_ALL` first, then `LANG`,
/// as every program on a Unix reads them, and `en-GB` when neither says.
fn system_locale() -> String {
    for name in ["LC_ALL", "LANG"] {
        let Ok(value) = std::env::var(name) else {
            continue;
        };
        let tag = language_tag(&value);
        if !tag.is_empty() {
            return tag;
        }
    }
    "en-GB".to_owned()
}

/// `en_GB.UTF-8` as a language tag, `en-GB`: what a POSIX locale looks like as
/// the tag every other program uses.
fn language_tag(locale: &str) -> String {
    let locale = locale.split('.').next().unwrap_or_default();
    let locale = locale.split('@').next().unwrap_or_default();
    if locale.is_empty() || locale == "C" || locale == "POSIX" {
        return String::new();
    }
    let mut parts = locale.split('_');
    let language = parts.next().unwrap_or_default().to_ascii_lowercase();
    let country = parts.next().map(str::to_ascii_uppercase);
    match country {
        Some(country) if !country.is_empty() => format!("{language}-{country}"),
        _ => language,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatMessage, Presence};
    use std::sync::Arc;

    /// A room as the sidebar lists it, which is what the application publishes.
    fn room(id: &str, name: &str) -> RoomView {
        RoomView {
            id: id.to_owned(),
            name: name.to_owned(),
            network: if id.starts_with("discord:") {
                yapfast_plugin_api::event::Network::Discord
            } else {
                yapfast_plugin_api::event::Network::Matrix
            },
            server: room_server(id),
            dm: false,
        }
    }

    fn message(id: &str, body: &str) -> Arc<ChatMessage> {
        Arc::new(ChatMessage {
            id: id.to_owned(),
            item: id.to_owned(),
            sender: "person-1".to_owned(),
            author: "Ann".to_owned(),
            body: body.to_owned(),
            timestamp: 1_700_000_000,
            ..ChatMessage::default()
        })
    }

    fn world() -> World {
        let world = World::new("0.3.0");
        world.publish_account(
            Some("person-1"),
            Some("Ann"),
            Some("ann"),
            None,
            Some("online"),
            false,
        );
        world.publish_lists(
            1,
            vec![
                room("discord:1:2", "general"),
                room("!matrix:example", "Matrix room"),
            ],
        );
        world.publish_selected(Some("discord:1:2"));
        world
    }

    #[test]
    fn the_account_and_the_lists_answer_as_the_application_knows_them() {
        let world = world();
        assert_eq!(world.app().version, "0.3.0");
        let me = world.me().expect("the account");
        assert_eq!(me.id, "person-1");
        assert_eq!(me.name, "Ann");
        assert_eq!(me.username.as_deref(), Some("ann"));

        let selected = world.selected().expect("the open room");
        assert_eq!(selected.id, "discord:1:2");
        assert_eq!(
            selected.server.as_deref(),
            Some("1"),
            "a channel names its server"
        );
        assert!(!selected.dm);

        let matrix = world.room("!matrix:example").expect("the other one");
        assert_eq!(matrix.server, None, "a Matrix room is nobody's channel");
        assert_eq!(
            world.rooms(None).len(),
            2,
            "the list is what the sidebar lists"
        );
        assert!(world.room("discord:9:9").is_none());
        assert_eq!(
            world.rooms(Some(1)).len(),
            1,
            "a plugin's own limit is kept"
        );
    }

    #[test]
    fn the_timeline_answers_a_page_and_one_message() {
        let world = world();
        let timeline = Arc::new(vec![
            message("1", "one"),
            message("2", "two"),
            message("3", "three"),
        ]);
        world.publish_timeline("discord:1:2", &timeline);

        let history = world
            .history("discord:1:2", None, None)
            .expect("the open room");
        let bodies: Vec<&str> = history.iter().map(|view| view.body.as_str()).collect();
        assert_eq!(bodies, ["one", "two", "three"], "the page is oldest first");
        assert_eq!(history[0].at, 1_700_000_000_000, "a time is milliseconds");
        assert_eq!(history[0].author.id, "person-1");

        let page = world
            .history("discord:1:2", Some("3"), Some(10))
            .expect("the open room");
        assert_eq!(page.len(), 2, "`before` stops at the message it names");
        assert_eq!(
            world.history("discord:1:3", None, None),
            None,
            "the application keeps one timeline, and it says so"
        );
        assert_eq!(
            world.message("discord:1:2", "2").map(|view| view.body),
            Some("two".to_owned())
        );
        assert!(world.message("discord:1:2", "9").is_none());
    }

    #[test]
    fn a_page_stops_at_one_frame_rather_than_being_cut() {
        let world = world();
        let long = "x".repeat(64 << 10);
        let timeline: Vec<Arc<ChatMessage>> = (0..MAX_MESSAGES * 2)
            .map(|index| message(&index.to_string(), &long))
            .collect();
        world.publish_timeline("discord:1:2", &Arc::new(timeline));
        let page = world
            .history("discord:1:2", None, None)
            .expect("the open room");
        let bytes = serde_json::to_string(&page).expect("it serializes").len();
        assert!(page.len() < MAX_MESSAGES, "the cap stops the page first");
        assert!(bytes <= ANSWER_BYTES, "{bytes} fits one frame");
    }

    #[test]
    fn a_member_is_answered_only_for_the_server_open() {
        let world = world();
        let member = ChatMember {
            id: "person-2".to_owned(),
            bot: false,
            name: "Bo".to_owned(),
            username: Some("bo".to_owned()),
            role: crate::chat::Role::Member,
            avatar: None,
            decoration: None,
            nameplate: None,
            style: None,
            group: None,
            group_icon: None,
            role_icon: None,
            presence: Some(Presence::DoNotDisturb),
            color: Some(0x00ff00),
            activity: Some("Listening to something".to_owned()),
            activities: crate::chat::Activities::default(),
            roles: Arc::from(vec!["7".to_owned()]),
            timed_out_until: None,
            tag: None,
            boosting_since: None,
        };
        world.publish_members("discord:1:2", &Arc::new(vec![member]));
        let found = world
            .member("1", "person-2")
            .open()
            .expect("the server is open")
            .expect("the member");
        assert_eq!(found.person.name, "Bo");
        assert_eq!(found.roles, ["7"]);
        assert_eq!(found.presence.as_deref(), Some("dnd"));
        assert_eq!(found.colour, Some(0x00ff00));
        assert!(
            world.member("2", "person-2").open().is_none(),
            "another server's list isn't here"
        );
        assert_eq!(
            world
                .member("1", "person-9")
                .open()
                .expect("the server is open"),
            None
        );
        assert_eq!(
            world.person("person-2").map(|person| person.name),
            Some("Bo".to_owned())
        );
        assert_eq!(
            world.person("person-1").map(|person| person.own),
            Some(true),
            "the account itself is known without a member list"
        );
    }

    #[test]
    fn publishing_what_has_not_changed_replaces_nothing() {
        let world = world();
        let rooms = world.rooms(None);
        world.publish_selected(Some("discord:1:2"));
        assert_eq!(world.rooms(None), rooms, "the list is the one already held");

        let timeline = Arc::new(vec![message("1", "one")]);
        world.publish_timeline("discord:1:2", &timeline);
        world.publish_timeline("discord:1:2", &timeline);
        assert_eq!(
            world
                .history("discord:1:2", None, None)
                .map(|page| page.len()),
            Some(1)
        );

        let other = Arc::new(vec![message("1", "one")]);
        world.publish_timeline("discord:1:2", &other);
        assert_eq!(
            world
                .history("discord:1:2", None, None)
                .map(|page| page.len()),
            Some(1),
            "a timeline that came anew is published"
        );
    }

    #[test]
    fn a_theme_is_the_interfaces_until_it_says_otherwise() {
        let world = World::new("0.3.0");
        assert_eq!(world.app().theme_mode, "dark");
        world.publish_theme(false);
        assert_eq!(world.app().theme_mode, "light");
    }

    #[test]
    fn a_posix_locale_becomes_a_language_tag() {
        assert_eq!(language_tag("en_GB.UTF-8"), "en-GB");
        assert_eq!(language_tag("de_DE@euro"), "de-DE");
        assert_eq!(language_tag("fr"), "fr");
        assert_eq!(language_tag("C.UTF-8"), "");
        assert_eq!(language_tag(""), "");
    }
}
