//! The conversation's header, as the Slint client's: a DM's avatar with
//! its presence or a channel's icon, a thread's channel leading back, the
//! name, a channel's topic on one line after a dot, what a DM's person is
//! up to in a bubble; then the room's buttons (call, threads,
//! notifications, pins, add people, the member list, the profile) and the
//! search box. The line under it is the conversation's.
//! The threads, pins and add people buttons open their popovers
//! (`popovers.rs`); the box searches (`search.rs`).

use std::sync::Arc;

use iced::{
    Alignment, Element, Length,
    widget::{Space, button, container, row, text, tooltip},
};
use yapfast_core::{
    application::{AppState, Relation},
    chat::{Presence, Topic},
};

use crate::{
    app::{App, Message},
    controls::{self, lit_style},
    fonts::{BOLD, MEDIUM},
    looks::Status,
    markdown::{self, Body},
    people::StatusInk,
    pictures::{Face, Icon},
    select::Piece,
    sidebar::room_icon,
    style::rounded,
    theme::rgb,
    widgets::Spot,
};

/// The header's height; the line under it is one more pixel.
pub const HEIGHT: f32 = 50.0;
/// A DM's avatar, and the icons before a name.
const AVATAR: f32 = 26.0;
const ROOM_ICON: u32 = 22;
pub const BUTTON: f32 = 32.0;
const GLYPH: u32 = 20;

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
    /// A Discord DM's person who isn't a friend: their id, and whether
    /// they asked to be one (Accept) or not yet (Add Friend). None once
    /// this account asked, which the profile card says.
    pub befriend: Option<(String, bool)>,
    /// A voice channel with a chat: whether the chat shows.
    pub voice_chat: Option<bool>,
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
        voice_chat: (!room.direct && room.voice_chat()).then_some(state.voice_chat),
        befriend: room
            .person
            .as_deref()
            .filter(|_| room.direct && room.capabilities.dm_calls)
            .and_then(|person| match state.relation(person) {
                Relation::Stranger if !state.is_blocked(person) => Some((person.to_owned(), false)),
                Relation::Incoming => Some((person.to_owned(), true)),
                _ => None,
            }),
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

/// Whether the member list shows beside the timeline.
pub fn members_shown(app: &App) -> bool {
    app.header.members_available
        && !app.roster.hidden
        && !crate::thread::aside_shown(app)
        && !crate::search::search_shown(app)
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

pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let header = ctx.header;
    let theme = ctx.look.theme;
    let mut head = row![].spacing(10).align_y(Alignment::Center);
    if header.direct {
        head = head.push(
            ctx.face(
                Face::new(header.avatar.as_deref(), &header.name, AVATAR).dot(
                    header
                        .presence
                        .map(|presence| (presence, rgb(theme.chat), iced::Point::new(14.0, 14.0))),
                ),
            ),
        );
    }
    if let Some((id, name, forum)) = &header.parent {
        let icon = if *forum { Icon::ForumPost } else { Icon::Hash };
        head = head
            .push(ctx.look.glyph(icon, ROOM_ICON, theme.muted))
            .push(
                button(text(name.as_str()).size(16).font(BOLD))
                    .padding(0)
                    .on_press(Message::Side(crate::sidebar::Event::Select(id.clone())))
                    .style(controls::ink_style(theme.secondary, theme.text)),
            )
            .push(ctx.look.glyph(Icon::ChevronRight, 16, theme.faint));
    }
    if let Some(icon) = header.icon {
        head = head.push(ctx.look.glyph(icon, ROOM_ICON, theme.muted));
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
        // A click shows it in full under the line, as Discord does;
        // its links open from there.
        let line = iced::widget::mouse_area(
            container(topic_line(ctx, topic))
                .width(Length::Fill)
                .clip(true),
        )
        .on_press(Message::Popover(crate::popovers::Event::Topic))
        .interaction(iced::mouse::Interaction::Pointer);
        head = head.push(dot(theme)).push(ctx.spot(line));
    } else if !header.activity.is_empty() {
        head = head.push(
            container(
                container(
                    ctx.status_line(&header.activity, StatusInk::under_name(ctx.look.theme, 1.0)),
                )
                .padding([0, 10])
                .height(24)
                .align_y(Alignment::Center)
                .style(move |_| rounded(theme.raised, 12)),
            )
            .width(Length::Fill),
        );
    }
    head = head.extend(header_buttons(ctx));
    container(head)
        .padding([0, 16])
        .width(Length::Fill)
        .height(HEIGHT)
        .align_y(Alignment::Center)
        .into()
}

/// The topic's one line, in the muted ink.
fn topic_line<'a>(ctx: crate::ctx::Ctx<'a>, topic: &'a Body) -> Element<'a, Message> {
    let ink = ctx.look.theme.muted;
    let spans = topic
        .parts
        .iter()
        .flat_map(|part| &part.runs)
        .map(|run| ctx.run_span(run, ink))
        .collect();
    Piece::new(spans, usize::MAX, 0, Message::Link, Message::Select)
        .unlined(yapfast_core::emoji::card::LINK)
        .size(14.0, 18.0)
        .color(ink)
        .starts(false)
        .into()
}

/// One of the header's buttons: its glyph lit while what it opens is
/// open, and where it is as it's pressed, which what it opens opens
/// from.
fn header_icon_button<'a>(
    ctx: crate::ctx::Ctx<'a>,
    icon: Icon,
    label: &'static str,
    message: Message,
    lit: bool,
) -> Element<'a, Message> {
    let ink = if lit {
        ctx.look.theme.text
    } else {
        ctx.look.theme.secondary
    };
    Spot::new(
        ctx.look
            .face_button(ctx.look.glyph(icon, GLYPH, ink), BUTTON, label, message)
            .below(),
        ctx.header_spot,
    )
    .into()
}

/// The buttons after the name, as the Slint client orders them.
fn header_buttons<'a>(ctx: crate::ctx::Ctx<'a>) -> Vec<Element<'a, Message>> {
    let header = ctx.header;
    let theme = ctx.look.theme;
    let shown = ctx.open.members;
    let members = if shown {
        "Hide member list"
    } else {
        "Show member list"
    };
    let mut buttons: Vec<Element<'a, Message>> = Vec::new();
    // Add Friend, or Accept Friend Request in the accent, first.
    if let Some((person, incoming)) = &header.befriend {
        let (words, kind) = if *incoming {
            ("Accept Friend Request", controls::Kind::Primary)
        } else {
            ("Add Friend", controls::Kind::Plain)
        };
        let ink = if *incoming {
            rgb(theme.on_accent)
        } else {
            rgb(theme.faint)
        };
        buttons.push(
            controls::action(
                theme,
                words,
                kind,
                Some(Message::Friends(crate::friends::Event::Accept(
                    person.clone(),
                ))),
            )
            .small()
            .icon(ctx.look.icon(Icon::UserPlus, 16, ink))
            .into(),
        );
    }
    // A voice channel's chat, or its call again.
    if let Some(chat) = header.voice_chat {
        let (icon, label) = if chat {
            (Icon::Volume, "Show voice channel")
        } else {
            (Icon::Forum, "Open chat")
        };
        buttons.push(
            ctx.look
                .face_button(
                    ctx.look.glyph(icon, GLYPH, theme.secondary),
                    BUTTON,
                    label,
                    Message::Side(crate::sidebar::Event::VoiceChat(!chat)),
                )
                .below()
                .into(),
        );
    }
    let mut push = |icon, label, message, lit| {
        buttons.push(header_icon_button(ctx, icon, label, message, lit));
    };
    if header.can_call {
        let label = if header.call_going {
            "Join Voice Call"
        } else {
            "Start Voice Call"
        };
        let call = |video| Message::Call(crate::call::Event::Start(video));
        push(Icon::Phone, label, call(false), false);
        if !header.call_going {
            push(Icon::Video, "Start Video Call", call(true), false);
        }
    }
    if header.can_threads {
        let open = ctx.state.thread_list().is_some();
        push(
            Icon::Thread,
            "Threads",
            Message::Popover(crate::popovers::Event::Threads),
            open,
        );
    }
    if header.can_notify {
        if header.muted {
            push(
                Icon::BellOff,
                "Notification settings, muted",
                Message::Bell,
                false,
            );
        } else {
            push(Icon::Bell, "Notification settings", Message::Bell, false);
        }
    }
    if header.can_pin {
        push(
            Icon::Pin,
            "Pinned messages",
            Message::Popover(crate::popovers::Event::Pins),
            false,
        );
    }
    if let Some(one_to_one) = header.add_people {
        let label = if one_to_one {
            "Add friends to DM"
        } else {
            "Add friends to the group"
        };
        push(
            Icon::UserPlus,
            label,
            Message::Popover(crate::popovers::Event::AddPeople),
            false,
        );
    }
    // The call's signal, in the conversation it's in, as in Discord's
    // voice panel.
    if let Some(signal) = header_signal(ctx) {
        buttons.push(signal);
    }
    let mut push = |icon, label, message, lit| {
        buttons.push(header_icon_button(ctx, icon, label, message, lit));
    };
    if header.members_available && !header.direct {
        push(Icon::Users, members, Message::ToggleMembers, shown);
    }
    // A thread's, after its member list, as in Discord's toolbar.
    if header.thread {
        push(
            Icon::Ellipsis,
            "More",
            Message::Popover(crate::popovers::Event::ThreadMore),
            false,
        );
    }
    if header.profile_available {
        let open = ctx.state.profile_panel;
        let label = if open { "Hide profile" } else { "Show profile" };
        push(
            Icon::User,
            label,
            Message::Profile(crate::profile::Event::TogglePanel),
            open,
        );
    }
    if header.members_available && header.direct {
        buttons.push(members_toggle(ctx, members, shown));
    }
    if let Some(placeholder) = &header.search {
        buttons.push(crate::search::header_search(
            ctx.searching,
            placeholder,
            ctx,
        ));
    }
    if !header.members_available && !header.profile_available && !header.members.is_empty() {
        buttons.push(
            row![
                ctx.look.glyph(Icon::Users, 18, theme.muted),
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

/// The call's signal in the conversation it's in, once it's
/// connected and measured: its ping under it, the details on a click.
fn header_signal<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let state = ctx.state;
    let voice = &state.voice;
    let here = voice.room.is_some() && voice.room == state.selected;
    if !here || !crate::call::voice_ready(voice) || !crate::call::details::link_measured(ctx) {
        return None;
    }
    let face = ctx.look.glyph(
        crate::call::signal_icon(voice.link_quality),
        GLYPH,
        crate::call::signal_ink(ctx.look.theme, voice.link_quality),
    );
    Some(crate::call::details::link_touch(
        ctx,
        face.boxed(BUTTON),
        true,
    ))
}

/// A group DM's member list toggle: its icon and count, as Slint's
/// `HeaderToggle`.
fn members_toggle<'a>(
    ctx: crate::ctx::Ctx<'a>,
    label: &'static str,
    open: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let ink = if open { theme.text } else { theme.secondary };
    let face = row![
        ctx.look.glyph(Icon::Users, GLYPH, ink),
        text(ctx.header.members.as_str())
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
