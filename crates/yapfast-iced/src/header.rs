//! The conversation's header: a DM's avatar with its presence or a
//! channel's icon, a thread's channel leading back, the name, a channel's
//! topic on one line after a dot, what a DM's person is up to in a bubble;
//! then the room's buttons (call, threads, notifications, pins, add people,
//! the member list, the profile) and the search box. The line under it is
//! the conversation's. The threads, pins and add people buttons open their
//! popovers (`header_popovers.rs`); the box searches (`search.rs`).

use std::sync::Arc;

use iced::{
    Alignment, Element, Length,
    widget::{Space, button, container, row, text, tooltip},
};
use yapfast_core::{
    application::{AppState, Relation},
    chat::{NotificationChange, Presence, Topic},
    i18n::{self, tr},
};

use crate::{
    app::{App, Message},
    controls::{self, lit_style},
    fonts::{MEDIUM, SEMIBOLD},
    looks::Status,
    markdown::{self, Body},
    people::StatusInk,
    pictures::{Face, GroupIcon, Icon, room_group_icon},
    piece::Piece,
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
pub const BUTTON: f32 = 30.0;
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
    pub group_icon: Option<GroupIcon>,
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
    /// Whether the call can be joined now: Discord online, no call
    /// joining or leaving. The call buttons are greyed while it can't.
    pub can_join: bool,
    /// A Discord DM's person who isn't a friend: their id, and whether
    /// they asked to be one (Accept) or not yet (Add Friend). None once
    /// this account asked, which the profile card says. The DM's opening
    /// offers it (Discord's header has no such button).
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
    // Discord's forum post has its bell too, which mutes the post.
    let post = parent.as_ref().is_some_and(|(_, _, forum)| *forum)
        && !state.preview
        && state.discord_user.is_some();
    Header {
        name: room.name.clone(),
        direct: room.direct,
        one_to_one: room.person.is_some(),
        thread: room.parent.is_some(),
        avatar: room.avatar.clone(),
        group_icon: room_group_icon(room),
        // As in the DM list: Discord sends the presence of friends and of
        // those sharing a server, and anyone else in a one-to-one DM shows
        // offline.
        presence: if room.person.is_some()
            && room.network == yapfast_core::network::Network::Discord
        {
            room.presence.or(Some(Presence::Offline))
        } else {
            room.presence
        },
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
        can_join: state.can_join_voice(&room.id),
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
        can_notify: post || state.menu_choices(&room.id).contains(&notify),
        muted: room.muted,
        can_pin: state
            .conversation_that(|can| can.pins)
            .is_some_and(|room| !room.forum),
        add_people: (state.discord_online && state.dm_that(|can| can.group_dms).is_some())
            .then_some(room.person.is_some()),
        members_available: state.lists_members(),
        members: match room.members {
            _ if room.forum => String::new(),
            0 => tr("Members").to_owned(),
            count => i18n::trn("{n} member", "{n} members", count as usize),
        },
        profile_available: state.one_to_one().is_some(),
        search: state.conversation_that(|can| can.search).map(|_| {
            let place = state.search_place();
            let looks_in = place
                .as_deref()
                .and_then(|place| state.servers.iter().find(|server| server.id == place))
                .map_or(room.name.as_str(), |server| server.name.as_str());
            i18n::fill(tr("Search {}"), &[&looks_in])
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
pub fn dot(theme: &crate::theme::Theme) -> Element<'static, Message> {
    let ink = if theme.bonfire {
        crate::style::hex(0x323238)
    } else {
        iced::Color {
            a: theme.faint.a * 0.75,
            ..theme.faint
        }
    };
    container(container(Space::new().width(4).height(4)).style(move |_| rounded(ink, 2)))
        .padding([0, 1])
        .width(6)
        .into()
}

pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let header = ctx.header;
    let theme = ctx.look.theme;
    let mut head = row![].spacing(10).align_y(Alignment::Center);
    if header.direct {
        head =
            head.push(
                ctx.face(
                    Face::new(header.avatar.as_deref(), &header.name, AVATAR)
                        .group(header.group_icon.as_ref().map(GroupIcon::as_face))
                        .dot(header.presence.map(|presence| {
                            (presence, rgb(theme.chat), iced::Point::new(14.0, 14.0))
                        })),
                ),
            );
    }
    if let Some((id, name, forum)) = &header.parent {
        let icon = if *forum { Icon::ForumPost } else { Icon::Hash };
        head = head
            .push(ctx.look.glyph(icon, ROOM_ICON, theme.channel_read))
            .push(
                button(text(name.as_str()).size(16).font(SEMIBOLD))
                    .padding(0)
                    .on_press(Message::Side(crate::sidebar::Event::Select(id.clone())))
                    .style(controls::ink_style(theme.secondary, theme.text)),
            )
            // Discord's chevron is a small one, close to the names it
            // parts.
            .push(ctx.look.glyph(Icon::ChevronRight, 10, theme.channel_read));
    }
    // A forum's post has no icon after its breadcrumb.
    let in_forum = header.parent.as_ref().is_some_and(|(_, _, forum)| *forum);
    let fills = header.topic.is_none() && header.activity.is_empty();
    // Discord sets the icon 7 px from the name that follows it, closer
    // than the 10 px between the header's other items.
    let mut title = row![].spacing(7).align_y(Alignment::Center);
    if let Some(icon) = header.icon.filter(|_| !in_forum) {
        title = title.push(ctx.look.glyph(icon, ROOM_ICON, theme.channel_read));
    }
    let name = container(
        text(header.name.as_str())
            .size(16)
            .font(SEMIBOLD)
            .color(theme.channel_white)
            .wrapping(text::Wrapping::None),
    )
    .clip(true);
    title = title.push(if fills {
        name.width(Length::Fill)
    } else {
        name
    });
    head = head.push(if fills {
        title.width(Length::Fill)
    } else {
        title
    });
    if let Some((_, topic)) = &header.topic {
        // A click shows it in full under the line, as Discord does;
        // its links open from there.
        let line = iced::widget::mouse_area(
            container(topic_line(ctx, topic))
                .width(Length::Fill)
                .clip(true),
        )
        .on_press(Message::Popover(crate::header_popovers::Event::Topic))
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
    // Measured in Discord: the icon starts 14 px in, the search box ends
    // 8 px from the window's edge.
    container(head)
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 14.0,
        })
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
    message: impl Into<Option<Message>>,
    lit: bool,
) -> Element<'a, Message> {
    // Without a message, greyed.
    let message = message.into();
    let ink = if lit {
        ctx.look.theme.channel_white
    } else if message.is_some() {
        ctx.look.theme.nav_read
    } else {
        ctx.look.theme.faint
    };
    Spot::new(
        ctx.look
            .face_button(ctx.look.glyph(icon, GLYPH, ink), BUTTON, label, message)
            .below(),
        ctx.header_spot,
    )
    .into()
}

/// The buttons after the name.
fn header_buttons<'a>(ctx: crate::ctx::Ctx<'a>) -> Vec<Element<'a, Message>> {
    let header = ctx.header;
    let theme = ctx.look.theme;
    let shown = ctx.open.members;
    let members = if shown {
        tr("Hide member list")
    } else {
        tr("Show member list")
    };
    let mut buttons: Vec<Element<'a, Message>> = Vec::new();
    // A voice channel's chat, or its call again.
    if let Some(chat) = header.voice_chat {
        let (icon, label) = if chat {
            (Icon::Volume, tr("Show voice channel"))
        } else {
            (Icon::Forum, tr("Open chat"))
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
    if header.can_call {
        let label = if header.call_going {
            tr("Join Voice Call")
        } else {
            tr("Start Voice Call")
        };
        let call = |video| {
            header
                .can_join
                .then(|| Message::Call(crate::call::Event::Start(video)))
        };
        buttons.push(header_icon_button(
            ctx,
            Icon::Phone,
            label,
            call(false),
            false,
        ));
        if !header.call_going {
            buttons.push(header_icon_button(
                ctx,
                Icon::Video,
                tr("Start Video Call"),
                call(true),
                false,
            ));
        }
    }
    let mut push = |icon, label, message: Message, lit| {
        buttons.push(header_icon_button(ctx, icon, label, message, lit));
    };
    if header.can_threads {
        let open = ctx.state.thread_list().is_some();
        push(
            Icon::Thread,
            tr("Threads"),
            Message::Popover(crate::header_popovers::Event::Threads),
            open,
        );
    }
    let place_menu = ctx
        .menu
        .filter(|menu| !menu.header && menu.place.is_some() && menu.place == ctx.state.selected);
    // The bell puts notification levels in the menu itself; the thread's
    // More and channel context menus put them in a submenu.
    let bell_open = place_menu.is_some_and(|menu| {
        menu.rows.iter().any(|row| {
            matches!(
                row.act,
                crate::menus::MenuAct::Notify(NotificationChange::Level(_))
            )
        })
    });
    if header.can_notify {
        if header.muted {
            push(
                Icon::BellOff,
                tr("Notification settings, muted"),
                Message::Bell,
                bell_open,
            );
        } else {
            push(
                Icon::Bell,
                tr("Notification settings"),
                Message::Bell,
                bell_open,
            );
        }
    }
    if header.can_pin {
        push(
            Icon::Pin,
            tr("Pinned messages"),
            Message::Popover(crate::header_popovers::Event::Pins),
            ctx.state.pins.is_some(),
        );
    }
    if let Some(one_to_one) = header.add_people {
        let label = if one_to_one {
            tr("Add friends to DM")
        } else {
            tr("Add friends to the group")
        };
        push(
            Icon::UserPlus,
            label,
            Message::Popover(crate::header_popovers::Event::AddPeople),
            ctx.state.add_people.is_some(),
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
        push(
            Icon::Users,
            members,
            Message::Members(crate::members::Event::Toggle),
            shown,
        );
    }
    // A thread's, after its member list, as in Discord's toolbar.
    if header.thread {
        push(
            Icon::Ellipsis,
            tr("More"),
            Message::Popover(crate::header_popovers::Event::ThreadMore),
            place_menu.is_some_and(|menu| ctx.header_spot.get().contains(menu.at)),
        );
    }
    if header.profile_available {
        let open = ctx.state.profile_panel;
        let label = if open {
            tr("Hide profile")
        } else {
            tr("Show profile")
        };
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
        // Discord's box sits 3 px further from the last button.
        buttons.push(
            container(crate::search::header_search(
                ctx.searching,
                placeholder,
                ctx,
            ))
            .padding(iced::Padding {
                left: 3.0,
                ..iced::Padding::ZERO
            })
            .into(),
        );
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

/// A group DM's member list toggle: its icon and count.
fn members_toggle<'a>(
    ctx: crate::ctx::Ctx<'a>,
    label: &'static str,
    open: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let ink = if open {
        theme.channel_white
    } else {
        theme.nav_read
    };
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
    .on_press(Message::Members(crate::members::Event::Toggle))
    .style(lit_style(None, theme.wash(0.1), 6.0));
    tooltip(face, controls::tip(theme, label), tooltip::Position::Bottom)
        .gap(6)
        .into()
}
