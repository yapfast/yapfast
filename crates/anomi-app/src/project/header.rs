//! The conversation's header from the core's state: its title, topic and
//! buttons. `show_header` and `show_dm_tools`' buttons in
//! `src/ui/refresh.rs` (`:2124`, `:2289`), with the header's parts of
//! `show_threads` (`:1921`), `show_composer` (`:4160`) and `show_voice`
//! (`:4612`), and the window's `friends-all`, the Friends tab chosen.
//!
//! A handful of fields, built again only when a [`Key`] of what they read
//! changed: who's speaking in a call reaches it and finds the key the same.
//! The topic's text is made again only when the topic changes, or the
//! pictures of its custom emoji arrive ([`topic`]). The popovers the
//! header's buttons open close here when the core closes what they show.

pub(crate) mod topic;

#[cfg(test)]
mod tests;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use yapfast_core::application::{AppState, DmScreen, MenuChoice, Navigation, Relation};
use yapfast_core::chat::{ChatRoom, Topic};
use yapfast_core::discord;
use yapfast_core::voice::Phase;
use yapfast_lang::{tr, trn};

use anomi_vm::header::{
    AddPeople, Call, Channel, Direct, Friend, Header, Members, Parent, Place, Screen, Title, Tools,
};
use anomi_vm::kind::Status;
use anomi_vm::shell::{Layer, Shell};

use super::rows;
use crate::layout::Layout;
use crate::pictures::Requests;

/// What the header showed last.
#[derive(Default)]
pub(super) struct Shown {
    key: Option<Key>,
    topic: Option<topic::Shown>,
}

/// Everything the header reads, compared before it is built again.
#[derive(PartialEq)]
struct Key {
    lists: u64,
    /// The pictures' count while the title shows pictures, else 0.
    pictures: u64,
    selection: u64,
    screen: Option<DmScreen>,
    browse: bool,
    navigation: u64,
    topic: Option<Arc<Topic>>,
    friend: Friend,
    phase: Phase,
    voice_here: bool,
    busy: bool,
    online: bool,
    preview: bool,
    voice_chat: bool,
    /// The pins, the threads and the friends picker, whether each is open.
    open: [bool; 3],
    members_hidden: bool,
    profile_hidden: bool,
    friends_all: bool,
}

impl Key {
    fn of(state: &AppState, layout: &Layout, friends_all: bool) -> Self {
        let room = state.selected_room();
        // A DM's picture and activity's emoji, a topic's custom emoji.
        let pictured = room.is_some_and(|room| room.direct)
            || state
                .topic
                .as_ref()
                .is_some_and(|topic| !topic.emoji.is_empty());
        let navigation = match &state.navigation {
            Navigation::DirectMessages => 0,
            Navigation::Server(id) => {
                let mut hasher = DefaultHasher::new();
                id.hash(&mut hasher);
                hasher.finish()
            }
        };
        Self {
            lists: state.list_changes,
            pictures: if pictured { state.picture_changes } else { 0 },
            selection: state.selection_version,
            screen: state.dm_screen,
            browse: state.browse.is_some(),
            navigation,
            topic: state.topic.clone(),
            friend: room.map_or(Friend::None, |room| friend(state, room)),
            phase: state.voice.phase,
            voice_here: voice_here(state),
            busy: state.busy,
            online: state.discord_online,
            preview: state.preview,
            voice_chat: state.voice_chat,
            // The core lets the threads go as the room closes.
            open: [
                pins_open(state),
                state.threads.is_some(),
                add_people_open(state),
            ],
            members_hidden: layout.members_hidden,
            profile_hidden: layout.profile_panel_hidden,
            friends_all,
        }
    }
}

/// Shows the header, and closes the popovers whose content the core let
/// go: the pins, the threads, the friends picker, and the topic once the
/// place changed.
pub(super) fn show(
    state: &AppState,
    layout: &Layout,
    friends_all: bool,
    shell: &mut Shell,
    header: &mut Header,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let key = Key::of(state, layout, friends_all);
    let Some(old) = shown.key.as_ref() else {
        return build(state, layout, key, header, shown, requests);
    };
    if *old == key {
        return;
    }
    let elsewhere = old.selection != key.selection || old.topic != key.topic;
    for (layer, open) in [Layer::Pins, Layer::Threads, Layer::AddPeople]
        .into_iter()
        .zip(key.open)
    {
        if !open && shell.is_open(layer) {
            shell.close(layer);
        }
    }
    if elsewhere && shell.is_open(Layer::Topic) {
        shell.close(Layer::Topic);
    }
    build(state, layout, key, header, shown, requests);
}

fn build(
    state: &AppState,
    layout: &Layout,
    key: Key,
    header: &mut Header,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let room = state.selected_room();
    let title = match room {
        Some(room) if room.direct => Title::Direct(direct(state, room, requests)),
        Some(room) => Title::Channel(channel(state, room, &mut shown.topic, requests)),
        None => screen(state, key.friends_all),
    };
    let tools = room.map_or_else(Tools::default, |room| tools(state, layout, room));
    *header = Header::Open(Box::new(Place { title, tools }));
    shown.key = Some(key);
}

/// A DM or a group: a one-to-one DM's person is a button, with what they
/// are up to beside it.
fn direct(state: &AppState, room: &ChatRoom, requests: &mut Requests) -> Direct {
    let activity = crate::language::once::line(&room.activities, room.activity.as_deref());
    let (emoji, words) = activity
        .as_deref()
        .map_or((None, ""), discord::status_parts);
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    requests.want(
        state,
        room.avatar.as_deref().into_iter().chain(emoji.as_deref()),
    );
    let activity = (!words.is_empty() || emoji.is_some()).then(|| Status {
        text: words,
        emoji: rows::expected(state, emoji.as_deref()),
        ..Status::default()
    });
    Direct {
        name: room.name.clone(),
        avatar: rows::avatar(state, &room.name, &room.id, room.avatar.as_deref()),
        presence: rows::presence(room.presence),
        person: state.one_to_one().is_some(),
        activity,
    }
}

/// A server's channel, a thread after its channel, or a forum post: a
/// forum's topic is its post guidelines, which its form shows.
fn channel(
    state: &AppState,
    room: &ChatRoom,
    shown: &mut Option<topic::Shown>,
    requests: &mut Requests,
) -> Channel {
    let parent = room
        .parent
        .as_deref()
        .and_then(|parent| state.rooms.iter().find(|listed| listed.id == parent));
    let (topic, line) = match state.topic.as_ref().filter(|_| !room.forum) {
        Some(topic) => topic::text(state, topic, shown, requests),
        None => {
            *shown = None;
            (None, None)
        }
    };
    Channel {
        icon: rows::channel_icon(room),
        name: room.name.clone(),
        parent: parent.map(|parent| Parent {
            name: parent.name.clone(),
            forum: parent.forum,
        }),
        topic,
        line,
    }
}

/// With no room open: Requests, Friends or Browse Channels, else the
/// sidebar's title.
fn screen(state: &AppState, friends_all: bool) -> Title {
    let home = state.navigation == Navigation::DirectMessages;
    match state.dm_screen {
        Some(DmScreen::Requests) if home => Title::Screen(Screen::Requests),
        Some(DmScreen::Friends) if home => Title::Screen(Screen::Friends { all: friends_all }),
        _ if state.browse.is_some() => Title::Screen(Screen::Browse),
        _ => Title::Sidebar(match &state.navigation {
            Navigation::DirectMessages => tr!("Direct messages").to_owned(),
            Navigation::Server(id) => state
                .servers
                .iter()
                .find(|server| &server.id == id)
                .map_or_else(|| tr!("Server").to_owned(), |server| server.name.clone()),
        }),
    }
}

/// The open room's buttons, each where Slint's header shows it.
fn tools(state: &AppState, layout: &Layout, room: &ChatRoom) -> Tools {
    // A Discord DM's call.
    let dm_call = room.direct && room.capabilities.dm_calls;
    // Discord counts a channel's members only once its list comes; a forum
    // has no member list.
    let count = match room.members {
        _ if room.forum => String::new(),
        0 => tr!("Members").to_owned(),
        count => trn!("{n} member", "{n} members", count),
    };
    Tools {
        friend: friend(state, room),
        voice_chat: room.voice_chat().then_some(state.voice_chat),
        call: (dm_call && !room.request && !voice_here(state)).then(|| Call {
            going: !room.people.is_empty(),
            can: state.can_join_voice(&room.id),
        }),
        threads: state
            .threads_channel()
            .map(|_| state.thread_list().is_some()),
        notify: state
            .menu_choices(&room.id)
            .contains(&MenuChoice::ChannelNotifications)
            .then_some(room.muted),
        pins: state
            .conversation_that(|can| can.pins)
            .filter(|room| !room.forum)
            .map(|_| pins_open(state)),
        add_people: (state.discord_online && state.dm_that(|can| can.group_dms).is_some()).then(
            || AddPeople {
                open: add_people_open(state),
                one_to_one: state.one_to_one().is_some(),
            },
        ),
        members: state.lists_members().then(|| Members {
            showing: !layout.members_hidden,
            label: count.clone(),
        }),
        more: room.parent.is_some(),
        profile: state.one_to_one().map(|_| !layout.profile_panel_hidden),
        search: state.conversation_that(|can| can.search).is_some(),
        count,
    }
}

/// Befriending a Discord DM's other person: offered to a stranger not
/// blocked, accepted from one who asked; a request sent shows on their
/// card, not here (`friend_offer`, `src/ui/profile.rs:200`).
fn friend(state: &AppState, room: &ChatRoom) -> Friend {
    let person = room
        .person
        .as_deref()
        .filter(|_| room.direct && room.capabilities.dm_calls);
    match person.map(|person| (person, state.relation(person))) {
        Some((person, Relation::Stranger)) if !state.is_blocked(person) => Friend::Add,
        Some((_, Relation::Incoming)) => Friend::Accept,
        _ => Friend::None,
    }
}

/// This account is in the open room's call.
fn voice_here(state: &AppState) -> bool {
    state.voice.active()
        && state.voice.room.is_some()
        && state.voice.room.as_deref() == state.selected.as_deref()
}

/// The open room's pinned messages show.
pub(crate) fn pins_open(state: &AppState) -> bool {
    state
        .pins
        .as_ref()
        .is_some_and(|pins| Some(pins.room.as_str()) == state.selected.as_deref())
}

/// The friends picker shows for the open DM.
pub(crate) fn add_people_open(state: &AppState) -> bool {
    state
        .add_people
        .as_ref()
        .is_some_and(|picker| Some(picker.room.as_str()) == state.selected.as_deref())
}
