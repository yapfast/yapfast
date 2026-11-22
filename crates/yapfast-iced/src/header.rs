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
    chat::{Presence, Topic},
};

use crate::{
    app::{App, Message},
    controls::{self, lit_style},
    fonts::{MEDIUM, SEMIBOLD},
    looks::Status,
    markdown::{self, Body},
    people::StatusInk,
    pictures::{Face, Icon},
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
    /// A Discord voice channel's chat opens beside its call, as Discord's
    /// does, rather than in its place.
    pub chat_beside: bool,
    pub can_threads: bool,
    pub can_notify: bool,
    pub muted: bool,
    pub can_pin: bool,
    /// Add people: to a DM (true) or a group.
    pub add_people: Option<bool>,
    pub members_available: bool,
    /// How many members the selected room reports, for theme chrome that
    /// writes the count beside its topic.
    pub members_n: u64,
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
        voice_chat: (!room.direct && room.voice_chat()).then_some(
            state.voice_chat
                || state
                    .aside
                    .as_ref()
                    .is_some_and(|aside| aside.voice && aside.room == room.id),
        ),
        chat_beside: room.network == yapfast_core::network::Network::Discord && !state.preview,
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
        members_n: room.members,
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

/// The narrowest window that shows the member list beside the timeline.
pub const MEMBERS_MIN_WINDOW: f32 = 1000.0;

/// Whether the member list shows beside the timeline.
pub fn members_shown(app: &App) -> bool {
    app.header.members_available
        && !app.roster.hidden
        // As Discord: a window too narrow to keep the conversation a
        // usable width hides the list (the header's button still opens
        // it once the window is wider).
        && app.window.width >= MEMBERS_MIN_WINDOW
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
            .push(ctx.look.glyph(icon, ROOM_ICON, theme.channel_read))
            .push(
                button(crate::theme_effects::label(
                    ctx,
                    yapfast_core::themes::package::art::TextRole::Headings,
                    name.clone(),
                    theme.header_size(16.0),
                    theme.header_font(SEMIBOLD),
                    theme.secondary,
                ))
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
    if let Some(icon) = header.icon.filter(|_| !in_forum) {
        head = head.push(ctx.look.glyph(icon, ROOM_ICON, theme.channel_read));
    }
    let fills = header.topic.is_none() && header.activity.is_empty();
    let header_name = ctx.themes.marked(
        &theme.id,
        yapfast_core::themes::chrome::Place::Header,
        &header.name,
    );
    let name = container(crate::theme_effects::slot(
        crate::theme_effects::label(
            ctx,
            yapfast_core::themes::package::art::TextRole::Headings,
            theme.heading(&header_name).into_owned(),
            theme.header_size(16.0),
            theme.header_font(SEMIBOLD),
            theme.surface_text("header").unwrap_or(theme.channel_white),
        ),
        ctx,
        yapfast_core::themes::package::Slot::ChannelName,
        None,
    ))
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
            container(labelled_topic_line(ctx, topic))
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
    if !header.direct
        && header.members_n > 0
        && let Some((template, role)) = ctx.themes.chrome(&theme.id).and_then(|chrome| {
            chrome
                .labels
                .get(&yapfast_core::themes::chrome::Label::MemberCount)
                .map(|template| {
                    (
                        template,
                        chrome
                            .font
                            .map_or("small", yapfast_core::themes::package::Role::name),
                    )
                })
        })
    {
        let count = header.members_n.to_string();
        let words = yapfast_core::themes::chrome::fill(template, &[("n", count.as_str())]);
        head = head.push(
            text(words)
                .size(theme.type_size(role, 14.0))
                .font(theme.font(role, MEDIUM))
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        );
    }
    // A theme may keep the bar's buttons out of sight until the pointer is
    // on the bar; they stay a hover and a click away.
    let shy = crate::theme_chrome::hides(ctx, yapfast_core::themes::chrome::Hide::HeaderButtons);
    let mut shy_buttons = None;
    if shy {
        shy_buttons = Some(
            container(
                row(header_buttons(ctx))
                    .spacing(10)
                    .align_y(Alignment::Center),
            )
            .padding([0, 16])
            .width(Length::Fill)
            .height(HEIGHT)
            .align_x(Alignment::End)
            .align_y(Alignment::Center),
        );
    } else {
        head = head.extend(header_buttons(ctx));
    }
    let border = theme.surface_border("header", 0.0, 0.0, iced::Color::TRANSPARENT);
    let shadow = theme.surface_shadow("header", iced::Shadow::default());
    let background = theme.surface_ground("header");
    let bar = container(head)
        .padding([0, 16])
        .width(Length::Fill)
        .height(HEIGHT)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background,
            border,
            shadow,
            ..Default::default()
        });
    match shy_buttons {
        Some(buttons) => crate::widgets::Hover::new(bar, buttons).into(),
        None => bar.into(),
    }
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
        .light(crate::theme_effects::text_light(
            ctx,
            yapfast_core::themes::package::art::TextRole::Body,
        ))
        .starts(false)
        .into()
}

/// The topic with an installed package's safe text before and after its
/// `{topic}` placeholder. The markdown body itself stays the same selectable
/// piece, so links and mentions keep Wired's behaviour.
fn labelled_topic_line<'a>(ctx: crate::ctx::Ctx<'a>, topic: &'a Body) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let Some((before, after)) = ctx
        .themes
        .chrome(&theme.id)
        .and_then(|chrome| {
            chrome
                .labels
                .get(&yapfast_core::themes::chrome::Label::Topic)
        })
        .and_then(|template| template.split_once("{topic}"))
    else {
        return topic_line(ctx, topic);
    };
    let role = ctx
        .themes
        .chrome(&theme.id)
        .and_then(|chrome| chrome.font)
        .map_or("small", yapfast_core::themes::package::Role::name);
    let fixed = |words: &'a str| {
        text(words)
            .size(theme.type_size(role, 14.0))
            .font(theme.font(role, MEDIUM))
            .color(theme.muted)
            .wrapping(text::Wrapping::None)
    };
    row![fixed(before), topic_line(ctx, topic), fixed(after)]
        .align_y(Alignment::Center)
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
        "Hide member list"
    } else {
        "Show member list"
    };
    let mut buttons: Vec<Element<'a, Message>> = Vec::new();
    // A voice channel's chat, or its call again.
    if let Some(chat) = header.voice_chat {
        let (icon, label) = match (chat, header.chat_beside) {
            (true, true) => (Icon::Forum, "Hide chat"),
            (false, true) => (Icon::Forum, "Show chat"),
            (true, false) => (Icon::Volume, "Show voice channel"),
            (false, false) => (Icon::Forum, "Open chat"),
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
            "Join Voice Call"
        } else {
            "Start Voice Call"
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
                "Start Video Call",
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
            "Threads",
            Message::Popover(crate::header_popovers::Event::Threads),
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
            Message::Popover(crate::header_popovers::Event::Pins),
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
            Message::Popover(crate::header_popovers::Event::AddPeople),
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
            "More",
            Message::Popover(crate::header_popovers::Event::ThreadMore),
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
                    .size(14.0)
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
            .size(14.0)
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
