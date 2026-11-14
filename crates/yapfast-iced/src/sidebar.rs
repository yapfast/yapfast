//! The sidebar beside the rail, as the Slint client's with its Discord look
//! (`RoomList`, `ServerHeader`, `CategoryHeading` and `ChannelRow` in
//! `ui/rail.slint`): on the DM home the search field, Friends and Requests,
//! the "Direct Messages" heading and each DM with its person's presence and
//! what they're up to; in a server its name (which opens its menu), then
//! its channels under their categories, which fold, with who is in each
//! voice channel under it. The list is virtualized: only rows near its
//! viewport are built.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use iced::{
    Alignment, Color, Element, Length, Padding, Point, Task,
    widget::{
        Space, button, column, container, mouse_area, row, scrollable, scrollable::Viewport, stack,
        text, text_input,
    },
};
use yapfast_core::{
    application::{Action, AppState, DmScreen, ListedCategory, Navigation},
    chat::{ChatRoom, MarkKind, Presence, VoicePerson},
    discord,
    network::Network,
};

use crate::{
    app::{App, Message, key},
    controls::{self, Count, bar, count_pill, lit_style},
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    looks::Status,
    people::{NameInk, Person},
    pictures::{Face, Icon},
    rail::count_badge,
    style::{corners, faded, rounded, scrollbar},
    sync::Scroll,
    timeline::MARGIN,
    widgets::{RightPress, Rows},
};

/// The sidebar's width, as the Slint client's Discord look sets it.
pub const WIDTH: f32 = 304.0;
/// Its header, the line under it included.
pub const HEADER: f32 = 50.0;
/// A channel's row, a DM's, a category's heading, someone in a voice
/// channel, and the gap after each row.
pub const ROW: f32 = 32.0;
const DM_ROW: f32 = 42.0;
const HEADING: f32 = 38.0;
const PERSON: f32 = 30.0;
const GAP: f32 = 2.0;
/// The space the list leaves after its last row.
const END: f32 = 12.0;
/// The list's inset from the sidebar's sides.
const INSET: f32 = 8.0;
/// A channel's count of mentions: 18 px, 11 px bold.
const ROW_COUNT: Count = Count {
    height: 18.0,
    size: 11.0,
    pad: 5.0,
};
/// The search field's id.
pub const FILTER: &str = "sidebar-filter";

/// What's done in the sidebar.
#[derive(Clone, Debug)]
pub enum Event {
    Scrolled(Viewport),
    /// The pointer came onto a row or left it, by its key.
    Point(String, bool),
    Select(String),
    /// A voice channel: joins its call and shows it.
    Voice(String),
    /// A category's heading, by what it folds by.
    Toggle(String),
    Filter(String),
    Friends,
    Requests,
    Browse,
    /// The server's name: its menu, below it.
    Header,
    /// A right click on a row, by its room id, where the pointer is.
    Menu(String, Point),
    /// Wave, in an empty DM, by its person.
    Wave(String),
}

/// A conversation's row, as the Slint client's `RoomRow` has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub icon: Icon,
    pub selected: bool,
    pub unread: bool,
    pub badge: String,
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
}

impl Room {
    /// The row's own height, without the people under it.
    fn row_height(&self) -> f32 {
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
            SideRow::Screens { .. } => 6.0 + ROW + GAP + ROW + 6.0,
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

/// The sidebar's header: the server's name and badge, or the DMs' search.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SideHeader {
    /// The open server, by id; none on the DM home.
    pub server: Option<String>,
    pub name: String,
    pub community: bool,
}

/// The header, from the core's state.
pub fn side_header(state: &AppState) -> SideHeader {
    match &state.navigation {
        Navigation::Server(id) => state
            .servers
            .iter()
            .find(|server| &server.id == id)
            .map_or_else(SideHeader::default, |server| SideHeader {
                server: Some(server.id.clone()),
                name: server.name.clone(),
                community: server.community,
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

/// The list's rows, from the core's state, as the Slint client lists them:
/// on the DM home Friends and Requests, the heading and the DMs (pinned
/// first), message requests and spam left to their screen; in a server
/// its pinned channels under their heading, then each category's heading
/// and channels, folded as `collapsed` says, a channel's threads after it.
/// `filter` keeps the conversations whose names hold it.
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
            badge: count_badge(room.mentions),
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

impl App {
    /// What's done in the sidebar.
    pub fn sidebar_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Scrolled(viewport) => {
                self.side_scroll = Scroll::of(viewport);
                return self.ask();
            }
            Event::Point(id, entered) => {
                crate::app::point(&mut self.pointed_side, id, entered);
                // A DM's nameplate and moving decoration come as it's
                // pointed at.
                return self.ask();
            }
            Event::Select(id) => self.client.dispatch(Action::SelectRoom(id)),
            // As the Slint client: the core's `SelectRoom` joins a voice
            // channel's call first, and shows it once in it.
            Event::Voice(id) => self.client.dispatch(Action::SelectRoom(id)),
            Event::Toggle(fold) => {
                self.lists_shown = None;
                let Navigation::Server(server) = self.client.state().navigation.clone() else {
                    return Task::none();
                };
                let collapsed = self.layout.collapsed.entry(server.clone()).or_default();
                if !collapsed.remove(&fold) {
                    collapsed.insert(fold);
                }
                if collapsed.is_empty() {
                    self.layout.collapsed.remove(&server);
                }
                crate::layout::store(self.client.root(), "collapsed", &self.layout.collapsed);
            }
            Event::Filter(filter) => {
                self.lists_shown = None;
                self.filter = filter;
            }
            Event::Friends => self
                .client
                .dispatch(Action::OpenDmScreen(DmScreen::Friends)),
            Event::Requests => self
                .client
                .dispatch(Action::OpenDmScreen(DmScreen::Requests)),
            Event::Browse => self.client.dispatch(Action::BrowseChannels),
            Event::Header => {
                let Some(server) = self.side_header.server.clone() else {
                    return Task::none();
                };
                if self.menu.as_ref().is_some_and(|menu| menu.header) {
                    self.menu = None;
                    return Task::none();
                }
                let at = Point::new(crate::rail::WIDTH + INSET, HEADER - 4.0);
                return self.open_place_menu(&server, at, true);
            }
            Event::Menu(id, at) => return self.open_place_menu(&id, at, false),
            Event::Wave(user) => self.client.dispatch(Action::MessageFromProfile {
                user,
                text: "\u{1F44B}".to_owned(),
            }),
        }
        self.sync()
    }

    pub fn sidebar_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let total: f32 = self.side.iter().map(SideRow::height).sum::<f32>() + END - GAP;
        let viewport = self.side_scroll;
        let mut rows = Rows::new(total);
        let mut top = 0.0;
        for row in &self.side {
            let height = row.height();
            let at = top;
            top += height;
            if at + height < viewport.offset - MARGIN
                || at > viewport.offset + viewport.height + MARGIN
            {
                continue;
            }
            rows.push(key(&row.key()), at, height, self.side_row(row));
        }
        let list = scrollable(rows)
            .on_scroll(|viewport| Message::Side(Event::Scrolled(viewport)))
            .height(Length::Fill)
            .style(scrollbar(theme));
        let ground = theme.sidebar;
        container(column![self.side_header_view(), list])
            .width(WIDTH)
            .height(Length::Fill)
            .clip(true)
            .style(move |_| rounded(ground, corners([12.0, 0.0, 0.0, 0.0])))
            .into()
    }

    /// The sidebar's top: a server's name, or the DMs' search field, over
    /// the outline.
    fn side_header_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let inner: Element<'_, Message> = match &self.side_header.server {
            None => {
                let (ground, ink, hint, selection) =
                    (theme.field, theme.text, theme.muted, theme.selection);
                text_input("Find a conversation", &self.filter)
                    .id(FILTER)
                    .on_input(|filter| Message::Side(Event::Filter(filter)))
                    .size(13)
                    .padding([8, 10])
                    .style(move |_, _| text_input::Style {
                        background: ground.into(),
                        border: iced::Border::default().rounded(6),
                        icon: ink,
                        placeholder: hint,
                        value: ink,
                        selection,
                    })
                    .into()
            }
            Some(_) => self.server_name_button(),
        };
        column![
            container(inner)
                .padding(INSET)
                .height(HEADER - 1.0)
                .width(Length::Fill),
            bar(theme.outline, Length::Fill, 1)
        ]
        .into()
    }

    /// A server's name at the top of its channels, as the Slint client's
    /// `ServerHeader`: its Community badge, the name and a chevron, lit
    /// under the pointer or while its menu is open, when the chevron turns.
    fn server_name_button(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let head = &self.side_header;
        let open = self.menu.as_ref().is_some_and(|menu| menu.header);
        let mut face = row![].spacing(10).align_y(Alignment::Center);
        if head.community {
            face = face.push(self.glyph(Icon::Community, 16, theme.text));
        }
        face = face.push(
            row![
                text(head.name.clone())
                    .size(15)
                    .font(SEMIBOLD)
                    .color(theme.text)
                    .wrapping(text::Wrapping::None),
                self.glyph(
                    if open {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    },
                    16,
                    theme.secondary
                ),
            ]
            .align_y(Alignment::Center),
        );
        let hover = theme.hover;
        button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding(Padding {
            left: if head.community { 10.0 } else { 8.0 },
            right: 8.0,
            ..Padding::ZERO
        })
        .height(32)
        .width(Length::Fill)
        .on_press(Message::Side(Event::Header))
        .style(lit_style(open.then_some(hover), hover, 6.0))
        .into()
    }

    fn side_row<'a>(&'a self, row: &'a SideRow) -> Element<'a, Message> {
        let theme = &self.theme;
        let inner: Element<'a, Message> = match row {
            SideRow::Screens {
                friends,
                requests,
                badge,
            } => column![
                self.link_row(
                    "screen:friends",
                    Icon::Users,
                    "Friends",
                    *friends,
                    "",
                    Message::Side(Event::Friends)
                ),
                self.link_row(
                    "screen:requests",
                    Icon::Inbox,
                    "Requests",
                    *requests,
                    badge,
                    Message::Side(Event::Requests)
                ),
            ]
            .spacing(GAP)
            .padding([6, 0])
            .into(),
            SideRow::DmHeading => row![
                text("Direct Messages")
                    .size(12)
                    .font(SEMIBOLD)
                    .color(theme.muted)
                    .width(Length::Fill),
                self.icon_button(
                    Icon::UserPlus,
                    "Add a friend",
                    24.0,
                    16,
                    Message::Nothing,
                    true
                ),
                self.icon_button(
                    Icon::SquarePen,
                    "New message",
                    24.0,
                    16,
                    Message::Nothing,
                    true
                ),
            ]
            .spacing(4)
            .padding(Padding {
                left: 8.0,
                ..Padding::ZERO
            })
            .height(40)
            .align_y(Alignment::Center)
            .into(),
            SideRow::Browse { open } => container(self.link_row(
                "screen:browse",
                Icon::Hash,
                "Browse channels",
                *open,
                "",
                Message::Side(Event::Browse),
            ))
            .padding([6, 0])
            .into(),
            SideRow::Pinned => container(
                text("PINNED CHANNELS")
                    .size(12)
                    .font(BOLD)
                    .color(theme.muted),
            )
            .padding(Padding {
                left: 16.0,
                ..Padding::ZERO
            })
            .height(36)
            .align_y(Alignment::Center)
            .into(),
            SideRow::PinnedEnd => container(bar(theme.line, Length::Fill, 1))
                .height(9)
                .align_y(Alignment::Center)
                .into(),
            SideRow::Heading {
                name,
                fold,
                collapsed,
            } => self.heading(name, fold, *collapsed),
            SideRow::Room(room) => self.room_rows(room),
            SideRow::Empty(words) => container(text(*words).size(14).color(theme.muted))
                .padding(8)
                .into(),
        };
        let inset = container(inner)
            .padding(Padding {
                left: INSET,
                right: INSET,
                bottom: GAP,
                top: 0.0,
            })
            .width(Length::Fill);
        match row {
            // Unread activity is marked at the sidebar's edge, as on the
            // rail, a thread's too.
            SideRow::Room(room) if room.unread && !room.selected => {
                let ink = theme.text;
                let mark = container(Space::new())
                    .width(4)
                    .height(8)
                    .style(move |_| rounded(ink, corners([0.0, 4.0, 4.0, 0.0])));
                stack![
                    inset,
                    controls::placed(mark, 0.0, (room.row_height() - 8.0) / 2.0)
                ]
                .into()
            }
            _ => inset.into(),
        }
    }

    /// A category's heading, as the Slint client's `CategoryHeading`: 17 px
    /// above its name, the chevron after it turned while folded; a click
    /// folds it or opens it.
    fn heading<'a>(
        &'a self,
        name: &'a str,
        fold: &'a str,
        collapsed: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let id = format!("heading:{fold}");
        let pointed = self.pointed_side.as_deref() == Some(id.as_str());
        let ink = if pointed {
            theme.secondary
        } else {
            theme.muted
        };
        let face = row![
            text(name)
                .size(14)
                .font(SEMIBOLD)
                .color(ink)
                .wrapping(text::Wrapping::None),
            self.glyph(
                if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                },
                12,
                ink
            ),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        mouse_area(
            container(face)
                .padding(Padding {
                    left: 16.0,
                    right: 8.0,
                    top: 17.0,
                    bottom: 0.0,
                })
                .height(HEADING)
                .width(Length::Fill)
                .align_y(Alignment::Center),
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .on_enter(Message::Side(Event::Point(id.clone(), true)))
        .on_exit(Message::Side(Event::Point(id, false)))
        .on_press(Message::Side(Event::Toggle(fold.to_owned())))
        .into()
    }

    /// A row that opens one of the DM home's screens or Browse Channels.
    fn link_row<'a>(
        &'a self,
        id: &'static str,
        icon: Icon,
        label: &'static str,
        selected: bool,
        badge: &str,
        message: Message,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let pointed = self.pointed_side.as_deref() == Some(id);
        let ink = if selected {
            theme.text
        } else if pointed {
            theme.secondary
        } else {
            theme.muted
        };
        let icon_ink = if selected { theme.text } else { theme.faint };
        let mut face = row![
            self.glyph(icon, 20, icon_ink),
            text(label)
                .size(15)
                .font(MEDIUM)
                .color(ink)
                .width(Length::Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !badge.is_empty() {
            face = face.push(count_pill(theme, badge, ROW_COUNT, None));
        }
        self.row_face(id.into(), face.into(), ROW, selected, message, None)
    }

    /// A row's face: lit while selected or pointed at, a click and a right
    /// click on it.
    fn row_face<'a>(
        &'a self,
        id: String,
        face: Element<'a, Message>,
        height: f32,
        selected: bool,
        message: Message,
        menu: Option<String>,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let pointed = self.pointed_side.as_deref() == Some(id.as_str());
        let ground = if selected {
            theme.active
        } else if pointed {
            theme.hover
        } else {
            Color::TRANSPARENT
        };
        let body = container(face)
            .padding([0, 8])
            .height(height)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| rounded(ground, 6));
        let area = mouse_area(body)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Side(Event::Point(id.clone(), true)))
            .on_exit(Message::Side(Event::Point(id, false)))
            .on_press(message);
        match menu {
            Some(room) => {
                let origin = iced::Vector::new(crate::rail::WIDTH, 0.0);
                RightPress::new(area, move |at| {
                    Message::Side(Event::Menu(room.clone(), at + origin))
                })
                .into()
            }
            None => area.into(),
        }
    }

    /// A conversation's row, with its thread line, and who is in it for a
    /// busy voice channel.
    fn room_rows<'a>(&'a self, room: &'a Room) -> Element<'a, Message> {
        let line = self.room_row(room);
        let line: Element<'a, Message> = if room.thread {
            row![self.thread_line(room.thread_continues), line].into()
        } else {
            line
        };
        if room.people.is_empty() {
            return line;
        }
        let mut people = column![].padding(Padding {
            left: 32.0,
            bottom: 4.0,
            ..Padding::ZERO
        });
        // The call this account is in shows who speaks.
        let voice = &self.client.state().voice;
        let call = voice.active() && voice.room.as_deref() == Some(room.id.as_str());
        for person in &room.people {
            let live = call
                .then(|| {
                    voice
                        .participants
                        .iter()
                        .find(|live| live.person == person.id || live.id == person.id)
                })
                .flatten();
            people = people.push(self.voice_person(&room.id, person, live));
        }
        column![line, people].spacing(GAP).into()
    }

    /// The line from a channel to its thread, 28 px wide: down from the
    /// channel and round into the thread, on down while another follows.
    fn thread_line(&self, continues: bool) -> Element<'_, Message> {
        let faint = self.theme.faint;
        let down = bar(faint, 1.5, if continues { ROW + GAP } else { 13.0 + GAP });
        let round = container(Space::new())
            .width(10.75)
            .height(7.5)
            .style(move |_| iced::widget::container::Style {
                border: iced::Border {
                    color: faint,
                    width: 1.5,
                    radius: corners([0.0, 0.0, 0.0, 6.0]),
                },
                ..Default::default()
            });
        container(stack![
            controls::placed(down, 17.25, 0.0),
            controls::placed(round, 17.25, 12.75),
        ])
        .width(28)
        .height(ROW)
        .into()
    }

    /// A conversation's row, as the Slint client's `ChannelRow`.
    fn room_row<'a>(&'a self, room: &'a Room) -> Element<'a, Message> {
        if room.direct {
            return self.dm_row(room);
        }
        let theme = &self.theme;
        let pointed = self.pointed_side.as_deref() == Some(room.id.as_str());
        let (ink, opacity) = self.row_ink(room, pointed);
        let mut face = row![].spacing(8).align_y(Alignment::Center);
        if !room.thread {
            let icon_ink = if room.selected {
                theme.text
            } else if room.unread {
                theme.secondary
            } else {
                theme.faint
            };
            face = face.push(self.glyph(room.icon, 20, faded(icon_ink, opacity)));
        }
        let name = text(room.name.as_str())
            .size(15)
            .font(MEDIUM)
            .color(ink)
            .wrapping(text::Wrapping::None);
        face = face.push(column![name].width(Length::Fill));
        if let Some(started) = room.started {
            let lasted = yapfast_core::clock::lasted(started, self.minute);
            face = face.push(text(lasted).size(12).color(theme.muted));
        }
        if !room.badge.is_empty() {
            face = face.push(count_pill(theme, &room.badge, ROW_COUNT, None));
        }
        let message = if room.voice {
            Message::Side(Event::Voice(room.id.clone()))
        } else {
            Message::Side(Event::Select(room.id.clone()))
        };
        self.row_face(
            room.id.clone(),
            face.into(),
            room.row_height(),
            room.selected,
            message,
            Some(room.id.clone()),
        )
    }

    /// A row's ink, and its opacity: a muted one is faint until it's
    /// selected or pointed at.
    fn row_ink(&self, room: &Room, pointed: bool) -> (Color, f32) {
        let theme = &self.theme;
        let ink = if room.locked {
            if room.selected {
                theme.muted
            } else {
                theme.faint
            }
        } else if room.selected || room.unread {
            theme.text
        } else if pointed {
            theme.secondary
        } else {
            theme.muted
        };
        let opacity = if room.muted && !room.selected && !pointed {
            0.5
        } else {
            1.0
        };
        (faded(ink, opacity), opacity)
    }

    /// A DM's row: the person's face with its decoration and presence, the
    /// name in its style with the server tag or OFFICIAL, what they're up
    /// to or the voice channel they're in, Wave for a DM nobody wrote in
    /// yet, the call, pin and mentions; the nameplate behind it while it's
    /// pointed at or selected, as Discord's client draws it only then.
    fn dm_row<'a>(&'a self, room: &'a Room) -> Element<'a, Message> {
        let theme = &self.theme;
        let pointed = self.pointed_side.as_deref() == Some(room.id.as_str());
        let (ink, opacity) = self.row_ink(room, pointed);
        let row_ground = if room.selected {
            theme.active
        } else if pointed {
            theme.hover
        } else {
            theme.sidebar
        };
        let face = Face {
            url: room.avatar.as_deref(),
            name: &room.name,
            size: 32.0,
            opacity,
            decoration: room.decoration.as_deref(),
            play: pointed,
            dot: room.presence.map(|presence| {
                (
                    presence,
                    crate::theme::rgb(row_ground),
                    Point::new(20.0, 20.0),
                )
            }),
        };
        let mut name = row![self.person_name(
            &room.name,
            NameInk::new(15.0, 500, ink).style(room.style.as_ref()),
        )]
        .spacing(4)
        .align_y(Alignment::Center);
        if let Some((letters, badge)) = &room.tag {
            name = name.push(self.tag_chip(
                letters,
                badge.as_deref(),
                crate::looks::Chip {
                    size: 12.0,
                    font: SEMIBOLD,
                    height: 16.0,
                    pad: 4.0,
                    radius: 4.0,
                    fill: crate::style::CHIP,
                    ink: theme.text,
                },
                opacity,
            ));
        }
        if room.official {
            name = name.push(self.official_tag());
        }
        let mut person = Person::new(face, name);
        // Their voice channel in full when nothing else is said, else a
        // speaker before what is, as the friends list has it.
        let spot = room
            .voice_spot
            .as_ref()
            .map(|(id, channel, server)| (id.as_str(), channel.as_str(), server.as_str()));
        let mut line = row![].spacing(4).align_y(Alignment::Center);
        if let Some(spot) = spot {
            line = line.push(self.voice_spot(spot, 12.0, !room.status.is_empty()));
        }
        if !room.status.is_empty() {
            line = line.push(self.status_line(&room.status, opacity));
        } else if room.official && spot.is_none() {
            line = line.push(
                text("Official Discord Message")
                    .size(12)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        if spot.is_some() || !room.status.is_empty() || room.official {
            person = person.sub(line);
        }
        let mut face = self.person(person);
        if let Some(user) = room.wave.as_ref().filter(|_| pointed || room.selected) {
            face = face.push(self.wave_button(user));
        }
        if room.call {
            face = face.push(self.glyph(Icon::Phone, 16, theme.live));
        }
        if room.pinned {
            face = face.push(self.glyph(Icon::Pin, 14, theme.faint));
        }
        if !room.badge.is_empty() {
            face = face.push(count_pill(theme, &room.badge, ROW_COUNT, None));
        }
        let plated: Element<'a, Message> = match &room.nameplate {
            Some((url, palette)) if pointed || room.selected => {
                let plate = self.nameplate(url, DM_ROW, pointed, room.selected, 1.0);
                crate::widgets::PlateRow::new(
                    face,
                    plate,
                    palette.map(crate::style::hex),
                    row_ground,
                    crate::members::plate_end(pointed, room.selected),
                    1.0,
                )
                .into()
            }
            _ => face.into(),
        };
        self.row_face(
            room.id.clone(),
            plated,
            room.row_height(),
            room.selected,
            Message::Side(Event::Select(room.id.clone())),
            Some(room.id.clone()),
        )
    }

    /// Discord's OFFICIAL badge by its own account's name: blurple, a
    /// check and the word.
    fn official_tag(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let accent = theme.accent;
        container(
            row![
                self.glyph(Icon::Check, 12, theme.on_accent),
                text("OFFICIAL").size(10).font(BOLD).color(theme.on_accent),
            ]
            .spacing(2)
            .align_y(Alignment::Center),
        )
        .padding([0, 4])
        .height(16)
        .align_y(Alignment::Center)
        .style(move |_| rounded(accent, 4))
        .into()
    }

    /// Wave, which Discord offers in a DM nobody has written in yet: it
    /// sends a waving hand.
    fn wave_button(&self, user: &str) -> Element<'_, Message> {
        let theme = &self.theme;
        let (rest, lit, on) = (theme.accent, theme.accent_hover, theme.on_accent);
        button(
            container(
                row![
                    self.glyph(Icon::Hand, 14, on),
                    text("Wave").size(12).font(MEDIUM).color(on),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([0, 8])
            .height(24)
            .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press(Message::Side(Event::Wave(user.to_owned())))
        .style(controls::lit_style(Some(rest), lit, 4.0))
        .into()
    }

    /// Someone in a voice channel, under its row: their avatar and name, and
    /// their camera, LIVE (which watches their stream), and mic and
    /// headphone marks. In the call this account is in, `live` says who
    /// speaks, ringed and bold, and who lost their connection, faint. A
    /// right click opens their voice menu.
    fn voice_person<'a>(
        &'a self,
        room: &str,
        person: &'a VoicePerson,
        live: Option<&yapfast_core::voice::Participant>,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let id = format!("person:{room}:{}", person.id);
        let pointed = self.pointed_side.as_deref() == Some(id.as_str());
        let opacity = if person.elsewhere && !pointed {
            0.5
        } else {
            1.0
        };
        let speaking = live.is_some_and(crate::call::speaking);
        let ink = if speaking {
            theme.text
        } else if live.is_some_and(|live| !live.connected) {
            theme.muted
        } else {
            theme.secondary
        };
        let avatar = self.avatar(person.avatar.as_deref(), &person.name, 22.0, 11.0, opacity);
        // The ring of someone speaking, round the avatar, outside it.
        let avatar: Element<'a, Message> =
            if speaking {
                let ring = theme.live;
                iced::widget::stack![
                    avatar,
                    iced::widget::pin(container(Space::new()).width(28).height(28).style(
                        move |_| iced::widget::container::Style {
                            border: iced::Border {
                                color: ring,
                                width: 2.0,
                                radius: 14.0.into(),
                            },
                            ..Default::default()
                        }
                    ))
                    .x(-3.0)
                    .y(-3.0),
                ]
                .into()
            } else {
                avatar
            };
        let mut face = row![
            avatar,
            text(person.name.as_str())
                .size(15)
                .font(if speaking { BOLD } else { MEDIUM })
                .color(faded(ink, opacity))
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !person.elsewhere {
            if person.video {
                face = face.push(self.glyph(Icon::Video, 16, theme.muted));
            }
            if person.streaming {
                let local = live.is_some_and(|live| live.local);
                let watch = (!local).then(|| {
                    Message::Call(crate::call::Event::ShowPerson(
                        room.to_owned(),
                        person.id.clone(),
                    ))
                });
                face = face.push(controls::live_pill(theme, watch));
            }
            if person.muted || person.deafened {
                let ink = if person.server_muted {
                    theme.danger
                } else {
                    theme.muted
                };
                face = face.push(self.glyph(Icon::MicOff, 16, ink));
            }
            if person.deafened {
                let ink = if person.server_deafened {
                    theme.danger
                } else {
                    theme.muted
                };
                face = face.push(self.glyph(Icon::HeadphoneOff, 16, ink));
            }
        }
        let ground = if pointed {
            theme.hover
        } else {
            Color::TRANSPARENT
        };
        let area = mouse_area(
            container(face)
                .padding(Padding {
                    left: 4.0,
                    right: 8.0,
                    ..Padding::ZERO
                })
                .height(PERSON)
                .width(Length::Fill)
                .align_y(Alignment::Center)
                .style(move |_| rounded(ground, 4)),
        )
        .on_press(Message::Call(crate::call::Event::Profile(
            person.id.clone(),
        )))
        .on_enter(Message::Side(Event::Point(id.clone(), true)))
        .on_exit(Message::Side(Event::Point(id, false)));
        let (room, user) = (room.to_owned(), person.id.clone());
        let origin = iced::Vector::new(crate::rail::WIDTH, 0.0);
        RightPress::new(area, move |at| {
            Message::Call(crate::call::Event::Person(
                room.clone(),
                user.clone(),
                at + origin,
            ))
        })
        .into()
    }
}
