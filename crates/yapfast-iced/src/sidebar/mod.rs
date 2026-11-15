//! The sidebar beside the rail, as the Slint client's with its Discord look
//! (`RoomList`, `ServerHeader`, `CategoryHeading` and `ChannelRow` in
//! `ui/rail.slint`): on the DM home the search field, Friends and Requests,
//! the "Direct Messages" heading and each DM with its person's presence and
//! what they're up to; in a server its name (which opens its menu), then
//! its channels under their categories, which fold, with who is in each
//! voice channel under it. The list is virtualized: only rows near its
//! viewport are built.

use iced::{
    Color, Element, Length, Point, Task,
    widget::{column, container, scrollable, scrollable::Viewport, stack},
};
use yapfast_core::application::{Action, DmScreen, Navigation};

use crate::{
    app::{App, Message, key},
    controls::{self},
    style::{corners, rounded, scrollbar},
    sync::MARGIN,
    sync::Scroll,
    widgets::Rows,
};

mod channels;
mod dms;
mod rows;
mod top;
mod voice;

pub use rows::{Room, SideHeader, SideRow, room_icon, side_header, sidebar};

/// The sidebar's width, as the Slint client's Discord look sets it.
pub const WIDTH: f32 = 304.0;
/// Its header, the line under it included.
pub const HEADER: f32 = 50.0;
/// A channel's row, a DM's, a category's heading, someone in a voice
/// channel, and the gap after each row.
pub const ROW: f32 = 32.0;
const DM_ROW: f32 = 42.0;
const HEADING: f32 = 38.0;
/// A row's own small button: a voice channel's Open chat, a category's "+".
const CHAT_BUTTON: f32 = 24.0;
const PERSON: f32 = 30.0;
const GAP: f32 = 2.0;
/// The space the list leaves after its last row.
const END: f32 = 12.0;
/// The list's inset from the sidebar's sides.
const INSET: f32 = 8.0;
/// The search field's id, and the list's.
pub const FILTER: &str = "sidebar-filter";
pub const LIST: &str = "sidebar-list";
/// How much taller the header is with the server's banner behind it, and
/// how far down the banner's shade reaches.
const BANNER: f32 = 81.0;
/// The boost goal's row, and the pill in it.
const BOOST_ROW: f32 = 39.0;
const BOOST_PILL: f32 = 26.0;
/// The boosts the goal counts to, as Discord's bar.
const BOOST_GOAL: u32 = 33;
/// Discord's boost pink, which the goal's fill fades in.
const BOOST_PINK: Color = Color {
    r: 1.0,
    g: 0x73 as f32 / 255.0,
    b: 0xfa as f32 / 255.0,
    a: 1.0,
};
/// The events row.
const EVENTS_ROW: f32 = 60.0;
/// A busy voice channel's card beside its row: its width, how far from
/// the row, and the people it names before "and N more".
const VOICE_CARD_WIDTH: f32 = 232.0;
const VOICE_CARD_GAP: f32 = 8.0;
const VOICE_CARD_PEOPLE: usize = 8;

/// The header's button that invites people, and the icon in it.
const INVITE: f32 = 32.0;

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
    /// The header's Invite people.
    Invite,
    /// The events row: the server's scheduled events, under it.
    Events,
    /// A right click on a row, by its room id, where the pointer is.
    Menu(String, Point),
    /// Wave, in an empty DM, by its person.
    Wave(String),
    /// A voice channel's chat, without joining its call.
    OpenChat(String),
    /// The voice channel open shows its chat (true) or its call.
    VoiceChat(bool),
    /// A category's "+": a channel made in it.
    CreateIn(String),
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
                if !entered {
                    self.voice_card_off = false;
                }
                crate::app::point(&mut self.pointed.side, id, entered);
                // A DM's nameplate and moving decoration come as it's
                // pointed at.
                return self.ask();
            }
            Event::Select(id) => self.client.dispatch(Action::SelectRoom(id)),
            // As the Slint client: the core's `SelectRoom` joins a voice
            // channel's call first, and shows it once in it.
            Event::Voice(id) => {
                self.voice_card_off = true;
                self.client.dispatch(Action::SelectRoom(id));
            }
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
            Event::Invite => {
                let Some(server) = self.side_header.server.clone() else {
                    return Task::none();
                };
                self.client.dispatch(Action::OpenInvite(server));
                return Task::batch([
                    self.sync(),
                    iced::widget::operation::focus(crate::app::NOBODY),
                ]);
            }
            Event::Events => {
                let Some(server) = self.side_header.server.clone() else {
                    return Task::none();
                };
                let top = self.side_top_height() - EVENTS_ROW;
                let at = Point::new(crate::rail::WIDTH + INSET, top + EVENTS_ROW - INSET);
                return self.open_events_menu(&server, at);
            }
            Event::Menu(id, at) => {
                self.voice_card_off = true;
                return self.open_place_menu(&id, at, false);
            }
            Event::Wave(user) => self.client.dispatch(Action::MessageFromProfile {
                user,
                text: "\u{1F44B}".to_owned(),
            }),
            Event::OpenChat(room) => self.client.dispatch(Action::OpenChat(room)),
            Event::VoiceChat(chat) => self.client.dispatch(Action::ShowVoiceChat(chat)),
            Event::CreateIn(category) => {
                return self.choose_place(
                    category,
                    yapfast_core::application::MenuChoice::CreateChannel,
                );
            }
        }
        self.sync()
    }

    pub fn sidebar_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let total: f32 = self.side.iter().map(SideRow::height).sum::<f32>() + END - GAP;
        let viewport = self.side_scroll;
        let mut rows = Rows::new(total);
        let mut top = 0.0;
        for (index, row) in self.side.iter().enumerate() {
            let height = row.height();
            let at = top;
            top += height;
            if at + height < viewport.offset - MARGIN
                || at > viewport.offset + viewport.height + MARGIN
            {
                continue;
            }
            let place = self.side_places.of_row.get(index).copied().flatten();
            rows.push(key(&row.key()), at, height, self.side_row(row, place));
        }
        let list = scrollable(rows)
            .id(LIST)
            .on_scroll(|viewport| Message::Side(Event::Scrolled(viewport)))
            .height(Length::Fill)
            .style(scrollbar(theme));
        // What's dragged follows the pointer, faint, over the list;
        // someone dragged from a voice channel, below it.
        let list: Element<'_, Message> = match self.moving_view() {
            Some((ghost, y)) => stack![list, controls::placed(ghost, 40.0, y)].into(),
            None => list.into(),
        };
        let list: Element<'_, Message> = match self.arranged_view() {
            Some((ghost, y)) => stack![
                list,
                controls::placed(
                    container(ghost).padding([0.0, INSET]),
                    0.0,
                    y.clamp(0.0, (viewport.height - ROW).max(0.0))
                )
            ]
            .into(),
            None => list,
        };
        let ground = theme.sidebar;
        let mut top = column![self.side_header_view()];
        if let Some(boosts) = self
            .side_header
            .boosts
            .filter(|_| self.side_header.server.is_some())
        {
            top = top.push(self.boost_goal(boosts));
        }
        if let Some(events) = self
            .side_header
            .events
            .filter(|_| self.side_header.server.is_some())
        {
            top = top.push(self.events_row(events));
        }
        container(column![top, list])
            .width(WIDTH)
            .height(Length::Fill)
            .clip(true)
            .style(move |_| rounded(ground, corners([12.0, 0.0, 0.0, 0.0])))
            .into()
    }

    /// How tall the sidebar's top is over its list: the header (taller
    /// with a banner), the boost goal and the events row.
    pub fn side_top_height(&self) -> f32 {
        let head = &self.side_header;
        if head.server.is_none() {
            return HEADER;
        }
        HEADER
            + if head.banner.is_some() { BANNER } else { 0.0 }
            + if head.boosts.is_some() {
                BOOST_ROW
            } else {
                0.0
            }
            + if head.events.is_some() {
                EVENTS_ROW
            } else {
                0.0
            }
    }
}
