//! The conversation's header, as the Slint client's: a DM's avatar with
//! its presence or a channel's icon, a thread's channel leading back, the
//! name, a channel's topic on one line after a dot, what a DM's person is
//! up to in a bubble; then the room's buttons (call, threads,
//! notifications, pins, add people, the member list, the profile) and the
//! search box. The line under it is the conversation's.
//!
//! The buttons that open what later milestones bring (calls from a DM,
//! threads, pins, the notifications menu, add people, the profile panel,
//! the search) show but do nothing yet.

use std::sync::Arc;

use iced::{
    Alignment, Element, Length,
    widget::{Space, button, container, mouse_area, row, text, tooltip},
};
use yapfast_core::{
    application::AppState,
    chat::{Presence, Topic},
};

use crate::{
    app::{App, Message},
    controls::{self, lit_style},
    fonts::{BOLD, MEDIUM},
    looks::Status,
    markdown::{self, Body},
    pictures::{Face, Icon},
    select::Piece,
    sidebar::room_icon,
    style::rounded,
    theme::rgb,
    widgets::Pic,
};

/// The header's height; the line under it is one more pixel.
pub const HEIGHT: f32 = 50.0;
/// A DM's avatar, and the icons before a name.
const AVATAR: f32 = 26.0;
const ROOM_ICON: u32 = 22;
const BUTTON: f32 = 32.0;
const GLYPH: u32 = 20;
/// The search box, as Slint's `HeaderSearch`.
const SEARCH_WIDTH: f32 = 240.0;
const SEARCH_HEIGHT: f32 = 30.0;

/// What the header shows of the open room.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub name: String,
    pub direct: bool,
    /// A DM with one person, not a group's.
    pub one_to_one: bool,
    /// A thread, under its channel.
    pub thread: bool,
    pub avatar: Option<String>,
    pub presence: Option<Presence>,
    pub icon: Option<Icon>,
    /// A thread's channel: its id, name, and whether it's a forum.
    pub parent: Option<(String, String, bool)>,
    /// The topic as the core shares it, and on one line, laid out once
    /// each time it changes.
    pub topic: Option<(Arc<Topic>, Arc<Body>)>,
    /// What a DM's person is up to.
    pub activity: Status,
    pub can_call: bool,
    pub call_going: bool,
    pub can_threads: bool,
    pub can_notify: bool,
    pub muted: bool,
    pub can_pin: bool,
    /// Add people: to a DM (true) or a group.
    pub add_people: Option<bool>,
    pub members_available: bool,
    pub members: String,
    pub profile_available: bool,
    pub search: Option<String>,
}

/// The header for `state`'s open room. The topic `was` shown is kept when
/// the core's is the same, so the topic is laid out only when it changes.
pub fn header(state: &AppState, was: &Header, link: u32) -> Header {
    let Some(room) = state.selected_room() else {
        return Header::default();
    };
    let parent = room
        .parent
        .as_deref()
        .and_then(|parent| state.rooms.iter().find(|listed| listed.id == parent))
        .map(|parent| (parent.id.clone(), parent.name.clone(), parent.forum));
    // A forum's topic is its post guidelines, which its form shows.
    let topic = state
        .topic
        .as_ref()
        .filter(|_| !room.forum)
        .map(|shown| match &was.topic {
            Some((topic, body)) if Arc::ptr_eq(topic, shown) => (topic.clone(), body.clone()),
            _ => (shown.clone(), Arc::new(one_line(shown, link))),
        });
    let dm_call = room.direct && room.capabilities.dm_calls && !room.request;
    let voice_here = state.voice.room.as_deref() == Some(room.id.as_str());
    let notify = yapfast_core::application::MenuChoice::ChannelNotifications;
    Header {
        name: room.name.clone(),
        direct: room.direct,
        one_to_one: room.person.is_some(),
        thread: room.parent.is_some(),
        avatar: room.avatar.clone(),
        presence: room.presence,
        icon: (!room.direct).then(|| room_icon(room)),
        parent,
        topic,
        activity: if room.direct {
            Status::of(room.activity.as_deref())
        } else {
            Status::default()
        },
        can_call: dm_call && !voice_here,
        call_going: !room.people.is_empty(),
        can_threads: state.threads_channel().is_some(),
        can_notify: state.menu_choices(&room.id).contains(&notify),
        muted: room.muted,
        can_pin: state
            .conversation_that(|can| can.pins)
            .is_some_and(|room| !room.forum),
        add_people: (state.discord_online && state.dm_that(|can| can.group_dms).is_some())
            .then_some(room.person.is_some()),
        members_available: state.lists_members(),
        members: match room.members {
            _ if room.forum => String::new(),
            0 => "Members".to_owned(),
            1 => "1 member".to_owned(),
            count => format!("{count} members"),
        },
        profile_available: state.one_to_one().is_some(),
        search: state.conversation_that(|can| can.search).map(|_| {
            let place = state.search_place();
            let looks_in = place
                .as_deref()
                .and_then(|place| state.servers.iter().find(|server| server.id == place))
                .map_or(room.name.as_str(), |server| server.name.as_str());
            format!("Search {looks_in}")
        }),
    }
}

/// A topic's Markdown on one line, as the header shows it.
fn one_line(topic: &Topic, link: u32) -> Body {
    let words = topic.body.split_whitespace().collect::<Vec<_>>().join(" ");
    // The mentions' places are in the topic as written; on one line they
    // move, so they're found again by their text.
    let mentions: Vec<_> = topic
        .mentions
        .iter()
        .map(|mention| yapfast_core::mention::Mentioned {
            at: words
                .match_indices(&mention.text)
                .map(|(at, _)| at)
                .collect(),
            ..mention.clone()
        })
        .collect();
    let mut body = markdown::parse(&words, &mentions, link, "topic", |_| true);
    for run in body.parts.iter_mut().flat_map(|part| &mut part.runs) {
        if run.text.contains('\n') {
            run.text = run.text.replace('\n', " ");
        }
    }
    body
}

impl App {
    pub fn header_view(&self) -> Element<'_, Message> {
        let header = &self.header;
        let theme = &self.theme;
        let mut head = row![].spacing(10).align_y(Alignment::Center);
        if header.direct {
            head =
                head.push(self.face(Face {
                    url: header.avatar.as_deref(),
                    name: &header.name,
                    size: AVATAR,
                    opacity: 1.0,
                    decoration: None,
                    play: false,
                    dot:
                        header.presence.map(|presence| {
                            (presence, rgb(theme.chat), iced::Point::new(14.0, 14.0))
                        }),
                }));
        }
        if let Some((id, name, forum)) = &header.parent {
            let icon = if *forum { Icon::ForumPost } else { Icon::Hash };
            head = head
                .push(self.glyph(icon, ROOM_ICON, theme.muted))
                .push(
                    button(text(name.as_str()).size(16).font(BOLD))
                        .padding(0)
                        .on_press(Message::Side(crate::sidebar::Event::Select(id.clone())))
                        .style(controls::ink_style(theme.secondary, theme.text)),
                )
                .push(self.glyph(Icon::ChevronRight, 16, theme.faint));
        }
        if let Some(icon) = header.icon {
            head = head.push(self.glyph(icon, ROOM_ICON, theme.muted));
        }
        let fills = header.topic.is_none() && header.activity.is_empty();
        let name = container(
            text(header.name.as_str())
                .size(16)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
        )
        .clip(true);
        head = head.push(if fills {
            name.width(Length::Fill)
        } else {
            name
        });
        if let Some((_, topic)) = &header.topic {
            head = head.push(dot(theme)).push(
                container(self.topic_line(topic))
                    .width(Length::Fill)
                    .clip(true),
            );
        } else if !header.activity.is_empty() {
            head = head.push(
                container(
                    container(self.status_line(&header.activity, 1.0))
                        .padding([0, 10])
                        .height(24)
                        .align_y(Alignment::Center)
                        .style(move |_| rounded(theme.raised, 12)),
                )
                .width(Length::Fill),
            );
        }
        head = head.extend(self.header_buttons());
        container(head)
            .padding([0, 16])
            .width(Length::Fill)
            .height(HEIGHT)
            .align_y(Alignment::Center)
            .into()
    }

    /// The topic's one line, in the muted ink.
    fn topic_line<'a>(&'a self, topic: &'a Body) -> Element<'a, Message> {
        let ink = self.theme.muted;
        let spans = topic
            .parts
            .iter()
            .flat_map(|part| &part.runs)
            .map(|run| self.run_span(run, ink))
            .collect();
        Piece::new(spans, usize::MAX, 0)
            .size(14.0, 18.0)
            .color(ink)
            .starts(false)
            .into()
    }

    /// The buttons after the name, as the Slint client orders them.
    fn header_buttons(&self) -> Vec<Element<'_, Message>> {
        let header = &self.header;
        let theme = &self.theme;
        let shown = self.members_shown();
        let members = if shown {
            "Hide member list"
        } else {
            "Show member list"
        };
        let mut buttons = Vec::new();
        let mut push = |icon, label, message, lit: bool| {
            let ink = if lit { theme.text } else { theme.secondary };
            buttons.push(self.icon_face_button(
                self.glyph(icon, GLYPH, ink),
                label,
                BUTTON,
                message,
                true,
            ));
        };
        if header.can_call {
            let label = if header.call_going {
                "Join Voice Call"
            } else {
                "Start Voice Call"
            };
            push(Icon::Phone, label, Message::Nothing, false);
            if !header.call_going {
                push(Icon::Video, "Start Video Call", Message::Nothing, false);
            }
        }
        if header.can_threads {
            push(Icon::Thread, "Threads", Message::Nothing, false);
        }
        if header.can_notify {
            if header.muted {
                push(
                    Icon::BellOff,
                    "Notification settings, muted",
                    Message::Nothing,
                    false,
                );
            } else {
                push(Icon::Bell, "Notification settings", Message::Nothing, false);
            }
        }
        if header.can_pin {
            push(Icon::Pin, "Pinned messages", Message::Nothing, false);
        }
        if let Some(one_to_one) = header.add_people {
            let label = if one_to_one {
                "Add friends to DM"
            } else {
                "Add friends to the group"
            };
            push(Icon::UserPlus, label, Message::Nothing, false);
        }
        if header.members_available && !header.direct {
            push(Icon::Users, members, Message::ToggleMembers, shown);
        }
        // A thread's, after its member list, as in Discord's toolbar.
        if header.thread {
            push(Icon::Ellipsis, "More", Message::Nothing, false);
        }
        if header.profile_available {
            push(Icon::User, "Show profile", Message::Nothing, false);
        }
        if header.members_available && header.direct {
            buttons.push(self.members_toggle(members, shown));
        }
        if let Some(placeholder) = &header.search {
            buttons.push(self.search_box(placeholder));
        }
        if !header.members_available && !header.profile_available && !header.members.is_empty() {
            buttons.push(
                row![
                    self.glyph(Icon::Users, 18, theme.muted),
                    text(header.members.as_str())
                        .size(14)
                        .font(MEDIUM)
                        .color(theme.muted)
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        buttons
    }

    /// A group DM's member list toggle: its icon and count, as Slint's
    /// `HeaderToggle`.
    fn members_toggle(&self, label: &'static str, open: bool) -> Element<'_, Message> {
        let theme = &self.theme;
        let ink = if open { theme.text } else { theme.secondary };
        let face = row![
            self.glyph(Icon::Users, GLYPH, ink),
            text(self.header.members.as_str())
                .size(14)
                .font(MEDIUM)
                .color(ink)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let face = button(
            container(face)
                .padding([0, 8])
                .height(BUTTON)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press(Message::ToggleMembers)
        .style(lit_style(None, theme.wash(0.1), 6.0));
        tooltip(face, controls::tip(theme, label), tooltip::Position::Bottom)
            .gap(6)
            .into()
    }

    /// The search box: its placeholder and icon in a sunken field. Typing
    /// in it comes with the search, in a later milestone.
    fn search_box<'a>(&'a self, placeholder: &'a str) -> Element<'a, Message> {
        let theme = &self.theme;
        let field = row![
            text(placeholder)
                .size(14)
                .color(theme.muted)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
            self.glyph(Icon::Search, GLYPH, theme.muted),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let line = theme.line;
        let ground = theme.sunken;
        mouse_area(
            container(field)
                .padding([0, 8])
                .width(SEARCH_WIDTH)
                .height(SEARCH_HEIGHT)
                .align_y(Alignment::Center)
                .clip(true)
                .style(move |_| container::Style {
                    border: iced::Border {
                        color: line,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..rounded(ground, 8)
                }),
        )
        .interaction(iced::mouse::Interaction::Text)
        .into()
    }

    /// Whether the member list shows beside the timeline.
    pub fn members_shown(&self) -> bool {
        self.header.members_available && !self.hide_members
    }

    /// An icon button as the Slint client's `IconButton`: a square face lit
    /// under the pointer, its glyph in the secondary ink, its label in the
    /// window's tooltip, below it for the header's (`tip-side: 2`), at once,
    /// as Slint shows it.
    pub fn icon_button(
        &self,
        icon: Icon,
        label: &'static str,
        size: f32,
        glyph: u32,
        message: Message,
        below: bool,
    ) -> Element<'_, Message> {
        let face = self.glyph(icon, glyph, self.theme.secondary);
        self.icon_face_button(face, label, size, message, below)
    }

    /// An icon button with a face of its own.
    pub fn icon_face_button(
        &self,
        face: Pic,
        label: &'static str,
        size: f32,
        message: Message,
        below: bool,
    ) -> Element<'_, Message> {
        let lit = self.theme.wash(0.1);
        let face = button(face.boxed(size))
            .padding(0)
            .width(size)
            .height(size)
            .on_press(message)
            .style(lit_style(None, lit, 6.0));
        let side = if below {
            tooltip::Position::Bottom
        } else {
            tooltip::Position::Top
        };
        tooltip(face, controls::tip(&self.theme, label), side)
            .gap(6)
            .into()
    }
}

/// Discord's small round dot between a name and its topic, with room
/// either side.
fn dot(theme: &crate::theme::Theme) -> Element<'static, Message> {
    let ink = iced::Color {
        a: theme.faint.a * 0.75,
        ..theme.faint
    };
    container(container(Space::new().width(3).height(3)).style(move |_| rounded(ink, 2)))
        .padding([0, 2])
        .width(9)
        .into()
}
