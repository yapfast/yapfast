//! The sidebar's rows as the core's lists make them, with no window in
//! them: each room's row, the headings, links and drop spaces between,
//! and the server's top.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use yapfast_core::{
    application::{AppState, Application, DmScreen, ListedCategory, MenuChoice, Navigation},
    chat::{ChatRoom, MarkKind, Permission, Presence, VoicePerson},
    discord,
    network::Network,
};

use crate::{controls::count_badge, looks::Status, pictures::Icon};

use super::{DM_ROW, GAP, HEADING, NAV_ROW, PERSON, ROW};

/// A conversation's row.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub icon: Icon,
    pub selected: bool,
    pub unread: bool,
    pub badge: String,
    /// A forum's unread posts, as its row counts them (empty for none).
    pub posts: String,
    pub muted: bool,
    pub locked: bool,
    pub voice: bool,
    /// A DM, with its picture, its person's presence and the line under it.
    pub direct: bool,
    pub avatar: Option<String>,
    pub presence: Option<Presence>,
    pub status: Status,
    /// A DM's person's looks: the decoration round the avatar, the
    /// nameplate's still and palette, the name's style, the server tag's
    /// letters and badge; Discord's own account; and a DM nobody has
    /// written in yet, which offers to wave, by its person.
    pub decoration: Option<Arc<str>>,
    pub nameplate: Option<(Arc<str>, Option<u32>)>,
    pub style: Option<crate::looks::NameLook>,
    pub tag: Option<(Arc<str>, Option<Arc<str>>)>,
    pub official: bool,
    pub wave: Option<String>,
    /// The server voice channel a one-to-one DM's person is in: its room
    /// id, its name and its server's.
    pub voice_spot: Option<(String, String, String)>,
    /// A DM with a call going on, and a DM pinned to the top.
    pub call: bool,
    pub pinned: bool,
    /// A thread under its channel, and whether another of its follows.
    pub thread: bool,
    pub thread_continues: bool,
    /// Who is in a voice channel, and since when (Unix seconds).
    pub people: Vec<VoicePerson>,
    pub started: Option<u64>,
    /// A voice channel with a chat of its own, which a button opens.
    pub chat: bool,
    /// A Discord DM's close button on its row, by its label: Close DM, or
    /// Leave Group for a group.
    pub close: Option<&'static str>,
}

impl Room {
    /// The row's own height, without the people under it.
    pub(super) fn row_height(&self) -> f32 {
        if self.direct { DM_ROW } else { ROW }
    }
}

/// A row of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum SideRow {
    /// Friends and Requests, at the top of the DMs.
    Screens {
        friends: bool,
        requests: bool,
        badge: String,
    },
    /// "Direct Messages", with its buttons.
    DmHeading,
    /// Browse Channels, at the top of a Community server's list.
    Browse {
        open: bool,
    },
    /// "PINNED CHANNELS" over a server's pinned channels, and the line after
    /// them.
    Pinned,
    PinnedEnd,
    /// A category's heading: its name, what it folds by, whether it's folded.
    Heading {
        name: String,
        fold: String,
        collapsed: bool,
    },
    Room(Box<Room>),
    /// What the list says when it has no rows.
    Empty(&'static str),
}

impl SideRow {
    pub fn key(&self) -> String {
        match self {
            SideRow::Screens { .. } => "screens".into(),
            SideRow::DmHeading => "dm-heading".into(),
            SideRow::Browse { .. } => "browse".into(),
            SideRow::Pinned => "pinned".into(),
            SideRow::PinnedEnd => "pinned-end".into(),
            SideRow::Heading { fold, .. } => format!("heading:{fold}"),
            SideRow::Room(room) => room.id.clone(),
            SideRow::Empty(_) => "empty".into(),
        }
    }

    /// The row's height, its gap after included.
    pub fn height(&self) -> f32 {
        GAP + match self {
            SideRow::Screens { .. } => 9.0 + NAV_ROW + GAP + NAV_ROW + 6.0,
            SideRow::DmHeading => 40.0,
            SideRow::Browse { .. } => 6.0 + ROW + 6.0,
            SideRow::Pinned => 36.0,
            SideRow::PinnedEnd => 9.0,
            SideRow::Heading { .. } => HEADING,
            SideRow::Room(room) => {
                room.row_height()
                    + if room.people.is_empty() {
                        0.0
                    } else {
                        GAP + room.people.len() as f32 * PERSON + 4.0
                    }
            }
            SideRow::Empty(_) => 40.0,
        }
    }
}

/// The sidebar's header: the server's name and badge, or the DMs' search;
/// and over a server's channels, its banner, boost goal and events row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SideHeader {
    /// The open server, by id; none on the DM home.
    pub server: Option<String>,
    pub name: String,
    pub community: bool,
    /// Verified or partnered: Discord's green badge before its name.
    pub verified: bool,
    /// Its Boost level, the number in the badge before its name.
    pub tier: Option<u8>,
    /// Its banner's address, which the header sits over.
    pub banner: Option<String>,
    /// This account may invite people to it, from the header's button.
    pub invite: bool,
    /// How many boosts it has, while its owner shows the progress bar.
    pub boosts: Option<u32>,
    /// How many scheduled events it has, while the events row shows: while
    /// it has some, or this account may schedule one, as Discord's.
    pub events: Option<usize>,
}

/// The header, from the core's state.
pub fn side_header(app: &Application) -> SideHeader {
    let state = &app.state;
    match &state.navigation {
        Navigation::Server(id) => state
            .servers
            .iter()
            .find(|server| &server.id == id)
            .map_or_else(SideHeader::default, |server| SideHeader {
                server: Some(server.id.clone()),
                name: server.name.clone(),
                community: server.community,
                verified: server.verified,
                tier: server.premium_tier,
                banner: server.banner.clone(),
                invite: app.header_choices(id).contains(&MenuChoice::Invite),
                boosts: server.boosts,
                events: (!server.events.is_empty() || app.may(id, Permission::CreateEvents))
                    .then_some(server.events.len()),
            }),
        Navigation::DirectMessages => SideHeader::default(),
    }
}

/// A conversation's icon, as the sidebar shows it: a lock for one this
/// account may not read, else its kind's, marked private or age-restricted.
pub fn room_icon(room: &ChatRoom) -> Icon {
    if room.locked {
        return Icon::Lock;
    }
    if room.direct {
        return Icon::Dm;
    }
    if room.parent.is_some() {
        return Icon::Thread;
    }
    let (plain, private, warning) = match room.mark.kind {
        MarkKind::Rules => return Icon::Rules,
        MarkKind::Stage => (Icon::Stage, Icon::StageLock, Icon::Stage),
        _ if room.voice => (Icon::Voice, Icon::VoiceLock, Icon::VoiceWarning),
        MarkKind::Media => (Icon::Media, Icon::MediaLock, Icon::MediaWarning),
        _ if room.forum => (Icon::Forum, Icon::ForumLock, Icon::ForumWarning),
        MarkKind::Announcement => (
            Icon::Announcement,
            Icon::AnnouncementLock,
            Icon::AnnouncementWarning,
        ),
        MarkKind::Plain => (Icon::Hash, Icon::HashLock, Icon::HashWarning),
    };
    if room.mark.age_restricted {
        warning
    } else if room.mark.private {
        private
    } else {
        plain
    }
}

/// The list's rows, from the core's state: on the DM home Friends and
/// Requests, the heading and the DMs (pinned first), message requests and
/// spam left to their screen; in a server its pinned channels under their
/// heading, then each category's heading and channels, folded as
/// `collapsed` says, a channel's threads after it. `filter` keeps the
/// conversations whose names hold it.
pub fn sidebar(
    state: &AppState,
    categories: &[ListedCategory],
    collapsed: &HashMap<String, BTreeSet<String>>,
    filter: &str,
    dark: bool,
) -> Vec<SideRow> {
    let cosmetics = state.cosmetics;
    let direct = state.navigation == Navigation::DirectMessages;
    let query = filter.to_lowercase();
    // Who is in a voice channel of a server this account is in, as the DM
    // list marks them.
    let spots = if direct {
        crate::people::voice_spots(state)
    } else {
        HashMap::new()
    };
    let mut listed: Vec<&ChatRoom> = state
        .scoped_rooms()
        .filter(|room| room.name.to_lowercase().contains(&query))
        .collect();
    // A forum's row counts its posts that are unread, and its badge the
    // mentions in them, as Discord's does. Only the row: the server's badge
    // counts each post already.
    let mut posts: HashMap<&str, (u64, u64)> = HashMap::new();
    for room in state.scoped_rooms() {
        if let Some(parent) = room.parent.as_deref()
            && (room.unread > 0 || room.mentions > 0)
        {
            let (unread, mentions) = posts.entry(parent).or_default();
            *unread += u64::from(room.unread > 0);
            *mentions += room.mentions;
        }
    }
    let mut rows = Vec::new();
    if direct {
        if state.discord_linked && !state.preview {
            rows.push(SideRow::Screens {
                friends: state.dm_screen == Some(DmScreen::Friends),
                requests: state.dm_screen == Some(DmScreen::Requests),
                badge: count_badge(crate::friends::waiting(state) as u64),
            });
        }
        if !state.preview {
            rows.push(SideRow::DmHeading);
        }
        listed.retain(|room| {
            !room.request && room.category != discord::REQUESTS && room.category != discord::SPAM
        });
        // Pinned DMs come first, in the order they were pinned.
        listed.sort_by_key(|room| {
            state
                .pinned
                .iter()
                .position(|id| *id == room.id)
                .unwrap_or(usize::MAX)
        });
    }
    let server = match &state.navigation {
        Navigation::Server(id) => state.servers.iter().find(|server| &server.id == id),
        Navigation::DirectMessages => None,
    };
    if server.is_some_and(|server| server.community) && state.discord_online {
        rows.push(SideRow::Browse {
            open: state.browse.is_some(),
        });
    }
    // A Discord server's pinned channels come first, each with its
    // threads, under a heading of their own, and leave their categories.
    let pinned = server.map_or(&[][..], |server| server.pinned.as_slice());
    let pinned_at = |room: &ChatRoom| {
        pinned
            .iter()
            .position(|id| *id == room.id || room.parent.as_ref() == Some(id))
    };
    let mut in_pinned = 0;
    if !pinned.is_empty() {
        listed.sort_by_key(|room| pinned_at(room).unwrap_or(usize::MAX));
        in_pinned = listed
            .iter()
            .take_while(|room| pinned_at(room).is_some())
            .count();
    }
    let folded = server
        .and_then(|server| collapsed.get(&server.id))
        .filter(|_| query.is_empty());
    let is_folded = |by: &str| folded.is_some_and(|folded| folded.contains(by));
    let voice_room = state
        .voice
        .active()
        .then_some(state.voice.room.as_deref())
        .flatten();
    // Categories with no channel show as their heading alone, where
    // Discord's order puts them among the others.
    let empty_heading = |category: &ListedCategory| SideRow::Heading {
        name: category.name.clone(),
        fold: category.id.clone(),
        collapsed: is_folded(&category.id),
    };
    let mut next_category = 0;
    let mut category: Option<(Option<&str>, &str)> = None;
    let (mut listed_text, mut listed_voice) = (false, false);
    // Whether the category the rows now come under is folded.
    let mut under_fold = false;
    for (index, room) in listed.iter().enumerate() {
        if !direct && index == 0 && in_pinned > 0 {
            rows.push(SideRow::Pinned);
        }
        if !direct && index == in_pinned && in_pinned > 0 {
            rows.push(SideRow::PinnedEnd);
        }
        if !direct && index >= in_pinned {
            let heading = if room.capabilities.categories {
                let this = (room.category_id.as_deref(), room.category.as_str());
                (category.replace(this) != Some(this) && !room.category.is_empty()).then(|| {
                    let by = room.category_id.as_deref().unwrap_or(&room.category);
                    (room.category.clone(), by.to_owned())
                })
            } else if room.voice && !listed_voice {
                listed_voice = true;
                Some(("VOICE CHANNELS".to_owned(), "VOICE CHANNELS".to_owned()))
            } else if !room.voice && !listed_text {
                listed_text = true;
                Some(("TEXT CHANNELS".to_owned(), "TEXT CHANNELS".to_owned()))
            } else {
                None
            };
            if let Some((name, by)) = heading {
                if query.is_empty()
                    && let Some(at) = categories[next_category..]
                        .iter()
                        .position(|known| known.id == by)
                {
                    rows.extend(
                        categories[next_category..next_category + at]
                            .iter()
                            .filter(|category| category.empty)
                            .map(empty_heading),
                    );
                    next_category += at + 1;
                }
                under_fold = is_folded(&by);
                rows.push(SideRow::Heading {
                    name,
                    fold: by,
                    collapsed: under_fold,
                });
            }
        }
        let selected = state.selected.as_ref() == Some(&room.id);
        let unread = room.unread > 0;
        // Under a folded category only the open channel, unread ones and
        // voice channels with people in them stay, as in Discord.
        if !direct
            && index >= in_pinned
            && under_fold
            && !(selected
                || unread
                || !room.people.is_empty()
                || voice_room == Some(room.id.as_str()))
        {
            continue;
        }
        let status = if room.direct && room.network == Network::Discord {
            if room.person.is_none() {
                Status::words(format!("{} members", room.members))
            } else {
                Status::of(room.activity.as_deref())
            }
        } else {
            Status::default()
        };
        rows.push(SideRow::Room(Box::new(Room {
            id: room.id.clone(),
            name: room.name.clone(),
            icon: room_icon(room),
            selected,
            unread,
            badge: count_badge(
                room.mentions
                    + posts
                        .get(room.id.as_str())
                        .filter(|_| room.forum)
                        .map_or(0, |&(_, mentions)| mentions),
            ),
            posts: posts
                .get(room.id.as_str())
                .filter(|_| room.forum)
                .map_or_else(String::new, |&(unread, _)| count_badge(unread)),
            muted: room.muted && !room.voice,
            locked: room.locked || room.join_locked,
            voice: room.voice,
            direct: room.direct,
            avatar: room.avatar.clone(),
            // Discord sends the presence of friends and of those sharing a
            // server; anyone else in a one-to-one DM shows offline.
            presence: if room.person.is_some() && room.network == Network::Discord {
                room.presence.or(Some(Presence::Offline))
            } else {
                room.presence
            },
            status,
            decoration: cosmetics
                .decoration(room.decoration.as_deref())
                .map(Arc::from),
            nameplate: room
                .nameplate
                .as_ref()
                .filter(|_| !state.hide_nameplates && room.direct)
                .map(|plate| {
                    let palette = plate.colors.map(|colors| colors[usize::from(!dark)]);
                    (plate.picture.clone(), palette)
                }),
            style: room
                .direct
                .then(|| {
                    crate::looks::name_look(
                        cosmetics.style(room.style.as_deref()),
                        &room.name,
                        false,
                        dark,
                    )
                })
                .flatten(),
            tag: cosmetics
                .tag(room.tag.as_ref())
                .map(|tag| (tag.text.clone(), tag.badge.clone())),
            official: room.official,
            wave: (room.direct && room.empty && !room.request)
                .then(|| room.person.clone())
                .flatten(),
            voice_spot: room
                .person
                .as_deref()
                .filter(|_| room.network == Network::Discord)
                .and_then(|person| spots.get(person))
                .map(|(room, channel, server)| {
                    (
                        (*room).to_owned(),
                        (*channel).to_owned(),
                        (*server).to_owned(),
                    )
                }),
            call: room.direct && !room.people.is_empty(),
            pinned: room.direct && state.pinned.contains(&room.id),
            thread: room.parent.is_some(),
            thread_continues: false,
            people: if room.voice && !room.direct {
                room.people.clone()
            } else {
                Vec::new()
            },
            started: room.voice_started.filter(|_| room.voice && !room.direct),
            chat: !room.direct && room.voice_chat(),
            close: (room.direct && room.network == Network::Discord).then_some(
                if room.person.is_some() {
                    "Close DM"
                } else {
                    "Leave Group"
                },
            ),
        })));
    }
    if !direct && query.is_empty() {
        rows.extend(
            categories[next_category.min(categories.len())..]
                .iter()
                .filter(|category| category.empty)
                .map(empty_heading),
        );
    }
    // A channel's threads follow it, so a thread's line runs on to the next
    // row when that's a thread too.
    for index in 1..rows.len() {
        let next_is_thread = matches!(&rows[index], SideRow::Room(room) if room.thread);
        if let SideRow::Room(room) = &mut rows[index - 1]
            && room.thread
            && next_is_thread
        {
            room.thread_continues = true;
        }
    }
    if !rows
        .iter()
        .any(|row| matches!(row, SideRow::Room(_) | SideRow::Heading { .. }))
    {
        let empty = if state.scoped_rooms().next().is_some() {
            "No matching conversations."
        } else if direct {
            "No direct messages yet. Existing DMs appear here after syncing."
        } else {
            "No channels in this server that you can see."
        };
        rows.push(SideRow::Empty(empty));
    }
    rows
}
