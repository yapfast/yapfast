//! The Discord friends list and the requests screen, in place of a
//! conversation on the DM home: the header's Online and All tabs and Add
//! Friend; the friends with their presence, voice channel and status, a
//! message button and their menu; friend and message requests with their
//! answers, those sent, and spam folded away; and the dialog that adds a
//! friend by username.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length, Padding, Point, Task,
    widget::{Space, button, column, container, mouse_area, operation, row, stack, text},
};
use yapfast_core::{
    application::{Action, AddFriend, DmScreen, Navigation},
    chat::{Friend, Presence},
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action, field, lit_style},
    dialog::dialog,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    looks::Status,
    people::StatusInk,
    people::{Person, voice_spots},
    pictures::{Face, Icon},
    style::{rounded, scrollbar},
    sync::Scroll,
    theme::rgb,
    widgets::{RightPress, Rows},
};

pub mod requests;

use requests::request_rows;
pub use requests::waiting;

/// A friend's row, and the list's room above its first.
const ROW: f32 = 62.0;
/// The search field's and the add dialog's field's ids.
pub const SEARCH: &str = "friend-search";
pub const ADD_FIELD: &str = "add-friend";
/// The add dialog's width.
const ADD_WIDTH: f32 = 460.0;
/// The friends list's id, which a switch of tab scrolls back to its top.
const LIST: &str = "friends";

/// What the friends and requests screens keep.
#[derive(Default)]
pub struct Friends {
    /// All friends, rather than those around.
    pub all: bool,
    pub filter: String,
    pub scroll: Option<Scroll>,
    pub spam_open: bool,
    /// The add dialog's username, and whether its last request went.
    pub add_name: String,
    add_sent: bool,
    /// The note about blocked or ignored accounts was closed.
    pub note_closed: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    All(bool),
    Filter(String),
    Scrolled(Scroll),
    /// Their DM, from the row or its button.
    Message(String),
    /// Their menu, by a right click on their row.
    Menu(String),
    Accept(String),
    Remove(String),
    /// A message request's conversation.
    Open(String),
    Spam,
    OpenAdd,
    /// Discord's note over the list was closed.
    DismissNote,
    /// Its link: the settings, where the blocked and ignored are.
    Settings,
    AddName(String),
    Send,
    CloseAdd,
}

/// The DM home's screen in place of a conversation, if one shows.
pub fn dm_screen(app: &App) -> Option<DmScreen> {
    let state = app.client.state();
    (state.navigation == Navigation::DirectMessages && state.selected.is_none())
        .then_some(state.dm_screen)
        .flatten()
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::All(all) => {
            // A tab shows its list in place of the Add Friend page.
            if app.client.state().add_friend.is_some() {
                app.client.dispatch(Action::CloseAddFriend);
            }
            app.friends.all = all;
            app.friends.scroll = None;
            return Task::batch([
                operation::scroll_to(
                    LIST,
                    iced::widget::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(0.0),
                    },
                ),
                app.ask(),
            ]);
        }
        Event::Filter(filter) => {
            app.friends.filter = filter;
            return app.ask();
        }
        Event::Scrolled(scroll) => {
            app.friends.scroll = Some(scroll);
            return app.ask();
        }
        Event::Message(user) => app.client.dispatch(Action::MessagePerson(user)),
        Event::Menu(user) => {
            let at = app.pressed.get();
            return crate::person_menu::open_person_menu(
                app,
                &user,
                at,
                crate::person_menu::CONTEXT_MENU,
            );
        }
        Event::Accept(user) => app.client.dispatch(Action::Befriend(user)),
        Event::Remove(user) => app.client.dispatch(Action::RemoveFriendRequest(user)),
        Event::Open(room) => {
            return app.update(Message::Side(crate::sidebar::Event::Select(room)));
        }
        Event::Spam => {
            app.friends.spam_open = !app.friends.spam_open;
            return app.ask();
        }
        Event::OpenAdd => {
            app.client.dispatch(Action::OpenAddFriend);
            app.friends.add_name.clear();
            app.friends.add_sent = false;
            return Task::batch([app.sync(), operation::focus(ADD_FIELD)]);
        }
        Event::DismissNote => app.friends.note_closed = true,
        Event::Settings => {
            return app.update(Message::Settings(crate::settings::Event::Reopen));
        }
        Event::AddName(name) => {
            app.friends.add_name = name;
            return Task::none();
        }
        Event::Send => {
            let name = app.friends.add_name.clone();
            if !name.trim().is_empty() {
                app.client.dispatch(Action::AddFriend(name));
            }
        }
        Event::CloseAdd => {
            app.client.dispatch(Action::CloseAddFriend);
            return Task::batch([app.sync(), crate::composer::refocus(app)]);
        }
    }
    app.sync()
}

/// A request that went clears the name, ready for the next.
pub fn friends_sync(app: &mut App) {
    let sent = app
        .client
        .state()
        .add_friend
        .as_ref()
        .is_some_and(|dialog| matches!(dialog.outcome, Some(Ok(_))));
    if sent && !app.friends.add_sent {
        app.friends.add_name.clear();
    }
    app.friends.add_sent = sent;
}

/// The pictures the friends and requests screens show: the avatars of
/// the friends in view, the requests', and Active Now's.
pub fn friends_pictures(app: &App) -> Vec<String> {
    let mut urls = Vec::new();
    match dm_screen(app) {
        Some(DmScreen::Friends) => {
            let shown = friends_shown(app.ctx());
            let scroll = app.friends.scroll.unwrap_or(Scroll::new(app.window.height));
            let (first, last) = scroll.range(shown.len(), ROW, 0.0);
            let cosmetics = app.client.state().cosmetics;
            for friend in &shown[first..last] {
                urls.extend(friend.avatar.clone());
                // The decoration's still, and the animated one it plays
                // while the friend is pointed at.
                if let Some(still) = cosmetics.decoration(friend.decoration.as_deref()) {
                    urls.push(crate::looks::animated_url(still));
                    urls.push(still.to_owned());
                }
            }
            urls.extend(crate::active_now::active_now_pictures(app));
        }
        Some(DmScreen::Requests) => {
            let rows = request_rows(app.client.state(), false);
            for request in rows.incoming.iter().chain(&rows.sent).chain(&rows.spam) {
                urls.extend(request.avatar.map(str::to_owned));
            }
        }
        None => {}
    }
    urls
}

/// A friend counts as around unless offline.
fn around(friend: &Friend) -> bool {
    friend
        .presence
        .is_some_and(|presence| presence != Presence::Offline)
}

/// A presence in words, as the friends list says it.
fn presence_word(presence: Presence) -> &'static str {
    match presence {
        Presence::DoNotDisturb => "Do Not Disturb",
        Presence::Idle => "Idle",
        Presence::Online | Presence::Mobile => "Online",
        _ => "Offline",
    }
}

/// Whether Discord's Add Friend page takes the friends list's place.
pub fn add_page(ctx: crate::ctx::Ctx<'_>) -> bool {
    let state = ctx.state;
    state.add_friend.is_some()
        && state.navigation == Navigation::DirectMessages
        && state.selected.is_none()
        && state.dm_screen == Some(DmScreen::Friends)
}

/// The header over the friends list or the requests: the screen's icon
/// and name, and on the friends list its tabs, Add Friend and the button
/// for a new group DM.
pub fn friends_header<'a>(ctx: crate::ctx::Ctx<'a>, screen: DmScreen) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (icon, name) = match screen {
        DmScreen::Friends => (Icon::Users, "Friends"),
        DmScreen::Requests => (Icon::Inbox, "Requests"),
    };
    let mut head = row![
        ctx.look.glyph(icon, 22, theme.channel_read),
        text(name)
            .size(16)
            .font(SEMIBOLD)
            .color(theme.channel_white)
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if screen == DmScreen::Friends {
        let all = ctx.friends.all;
        let page = add_page(ctx);
        // The tabs sit 16 px apart, 6 px more than the header's spacing.
        let apart = |tab| container(tab).padding(Padding::ZERO.right(6));
        head = head
            .push(crate::header::dot(theme))
            .push(apart(header_tab(
                ctx,
                "Online",
                !all && !page,
                false,
                Some(Message::Friends(Event::All(false))),
            )))
            .push(apart(header_tab(
                ctx,
                "All",
                all && !page,
                false,
                Some(Message::Friends(Event::All(true))),
            )))
            .push(apart(header_tab(
                ctx,
                "Pending",
                false,
                false,
                Some(Message::Side(crate::sidebar::Event::Requests)),
            )));
        head = if page {
            head.push(header_tab(ctx, "Add Friend", true, true, None))
        } else {
            head.push(
                action(
                    theme,
                    "Add Friend",
                    Kind::Primary,
                    Some(Message::Friends(Event::OpenAdd)),
                )
                .small()
                .height(32.0),
            )
        };
        head = head.push(Space::new().width(Length::Fill));
        // Discord's header ends with the button for a new group DM, which
        // opens the same chooser as a new message.
        if !ctx.state.preview {
            head = head.push(
                container(
                    ctx.look
                        .icon_button(
                            Icon::MessageCirclePlus,
                            "New Group DM",
                            Message::Switcher(crate::switcher::Event::OpenNewDm),
                        )
                        .size(32.0, 20)
                        .ink(theme.nav_read)
                        .below(),
                )
                .padding(Padding::ZERO.right(8).top(2)),
            );
        }
    } else {
        head = head.push(Space::new().width(Length::Fill));
    }
    container(head)
        .padding([0, 16])
        .width(Length::Fill)
        .height(crate::header::HEIGHT)
        .align_y(Alignment::Center)
        .into()
}

/// A tab in the friends header: 32 px tall, the chosen one a step lighter
/// (the Add Friend page's tab a pill tinted with the accent).
fn header_tab<'a>(
    ctx: crate::ctx::Ctx<'a>,
    label: &'static str,
    chosen: bool,
    brand: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (text_ink, secondary, accent, hover) =
        (theme.text, theme.secondary, theme.accent, theme.hover);
    let chosen_ground = if theme.bonfire {
        crate::style::hex(0x323238)
    } else {
        theme.active
    };
    button(
        container(text(label).size(15).font(MEDIUM))
            .padding([0, 12])
            .height(32)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press_maybe(message)
    .style(move |_, status| {
        let lit = controls::lights(status);
        button::Style {
            background: if chosen && brand {
                Some(crate::style::faded(accent, 0.2).into())
            } else if chosen {
                Some(chosen_ground.into())
            } else if lit {
                Some(hover.into())
            } else {
                None
            },
            text_color: if chosen && brand {
                accent
            } else if chosen || lit {
                text_ink
            } else {
                secondary
            },
            border: crate::style::round(6),
            ..Default::default()
        }
    })
    .into()
}

/// The friends list, or the Add Friend page, beside Active Now when
/// there's room for it.
pub fn friends_screen<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let wide = crate::active_now::active_now_fits(ctx);
    let list = if add_page(ctx) {
        add_friend_page(ctx)
    } else {
        friends_list(ctx)
    };
    if !wide {
        return list;
    }
    row![
        container(list).width(Length::Fill).clip(true),
        crate::active_now::view(ctx)
    ]
    .into()
}

fn friends_list<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let state = ctx.state;
    if state.friends.is_empty() {
        return ctx.look.empty_screen(
            Icon::Users,
            "No friends yet",
            "Add someone with their Discord username.",
            Some(("Add a friend", Message::Friends(Event::OpenAdd))),
        );
    }
    let shown = friends_shown(ctx);
    // Discord's field: a 1 px border, a magnifier, 40 px in all.
    let (ink, hint, selection) = (theme.text, theme.muted, theme.selection);
    let input = iced::widget::text_input("Search", &ctx.friends.filter)
        .id(iced::widget::Id::new(SEARCH))
        .size(15)
        .padding(0)
        .on_input(|text| Message::Friends(Event::Filter(text)))
        .style(move |_, _| iced::widget::text_input::Style {
            background: iced::Color::TRANSPARENT.into(),
            border: iced::Border::default(),
            icon: ink,
            placeholder: hint,
            value: ink,
            selection,
        });
    let (ground, line) = if theme.bonfire {
        (crate::style::hex(0x16161a), crate::style::hex(0x303035))
    } else {
        (theme.sunken, iced::Color::TRANSPARENT)
    };
    let search = container(
        row![ctx.look.glyph(Icon::Search, 16, theme.secondary), input]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([0, 14])
    .height(40)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| crate::style::outlined(ground, line, 8.0));
    // Discord's heading is white, mixed case, set lower, with an en dash.
    let heading = container(
        text(format!(
            "{} \u{2014} {}",
            if ctx.friends.all {
                "All Friends"
            } else {
                "Online"
            },
            shown.len()
        ))
        .size(14)
        .font(SEMIBOLD)
        .color(theme.text),
    )
    .height(42)
    .padding(Padding::ZERO.top(7))
    .align_y(Alignment::Center);
    // Discord's list runs to the Active Now divider, so its scroll bar
    // sits there; the field, note and heading keep 16 px off it themselves.
    let mut page = column![].spacing(8).padding(Padding {
        top: 16.0,
        left: 24.0,
        right: 0.0,
        bottom: 0.0,
    });
    // Discord's note shows once some account is blocked or ignored.
    let anyone = !ctx.state.discord_blocked.is_empty() || !ctx.state.discord_ignored.is_empty();
    if anyone && !ctx.friends.note_closed {
        page = page.push(blocked_note(ctx)).push(Space::new().height(4));
    }
    page = page
        .push(container(search).padding(Padding::ZERO.right(16)))
        .push(heading)
        .height(Length::Fill);
    if shown.is_empty() {
        let words = if ctx.friends.filter.is_empty() {
            "None of your friends are around right now."
        } else {
            "No friend's name matches that."
        };
        page = page.push(container(text(words).size(14).color(theme.secondary)).padding([0, 8]));
    }
    let scroll = ctx
        .friends
        .scroll
        .unwrap_or(Scroll::new(ctx.look.window.height));
    let (first, last) = scroll.range(shown.len(), ROW, 0.0);
    let total = shown.len() as f32 * ROW + 16.0;
    let spots = voice_spots(state);
    let mut rows = Rows::new(total);
    for (index, friend) in shown.iter().enumerate().take(last).skip(first) {
        rows.push(
            crate::app::key(&friend.id),
            index as f32 * ROW,
            ROW,
            container(friend_row(
                ctx,
                friend,
                spots.get(friend.id.as_str()).copied(),
            ))
            .padding(Padding::ZERO.right(16)),
        );
    }
    let list = scroll_view(rows)
        .id(iced::widget::Id::new(LIST))
        .on_scroll(|viewport| Message::Friends(Event::Scrolled(Scroll::of(viewport))))
        .height(Length::Fill)
        .style(scrollbar(theme));
    page.push(list).into()
}

/// Discord's note over the friends: accounts blocked or ignored are in
/// the settings. It can be closed.
fn blocked_note<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let link = button(
        text("Go to Settings")
            .size(15)
            .font(SEMIBOLD)
            .color(theme.link),
    )
    .padding(0)
    .on_press(Message::Friends(Event::Settings))
    .style(controls::ink_style(theme.link, theme.link));
    let note = container(
        row![
            ctx.look.glyph(Icon::CircleInfoFilled, 20, theme.secondary),
            row![
                text("Looking for accounts you've blocked or ignored?")
                    .size(15)
                    .font(SEMIBOLD)
                    .color(theme.text),
                link
            ]
            .spacing(4)
            .width(Length::Fill),
            ctx.look
                .icon_button(Icon::X, "Dismiss", Message::Friends(Event::DismissNote))
                .size(28.0, 18)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(Padding::ZERO.left(16).right(12))
    .height(59)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| rounded(theme.sunken, 8.0));
    container(note)
        .padding(Padding::ZERO.left(6).right(20))
        .into()
}

/// Discord's Add Friend page, in place of the list while its tab is
/// chosen: the title, then the username and its button in one box.
fn add_friend_page<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let Some(dialog_state) = ctx.state.add_friend.as_ref() else {
        return Space::new().into();
    };
    let sending = dialog_state.sending;
    let name = &ctx.friends.add_name;
    let ready = !sending && !name.is_empty();
    let (ink, hint, selection) = (theme.text, theme.muted, theme.selection);
    let mut input = iced::widget::text_input("Enter a username", name)
        .id(iced::widget::Id::new(ADD_FIELD))
        .size(15)
        .padding(12)
        .width(Length::Fill)
        .on_input(|text| Message::Friends(Event::AddName(text)))
        .style(move |_, _| iced::widget::text_input::Style {
            background: iced::Color::TRANSPARENT.into(),
            border: iced::Border::default(),
            icon: ink,
            placeholder: hint,
            value: ink,
            selection,
        });
    if ready {
        input = input.on_submit(Message::Friends(Event::Send));
    }
    let send = action(
        theme,
        "Send Friend Request",
        Kind::Primary,
        ready.then_some(Message::Friends(Event::Send)),
    )
    .small()
    .height(32.0);
    let (ground, line) = if theme.bonfire {
        (crate::style::hex(0x16161a), crate::style::hex(0x303035))
    } else {
        (theme.sunken, theme.line)
    };
    // 56 px inside its 1 px border, the button 12 px from the right edge.
    let field = container(
        row![input, send]
            .spacing(12)
            .padding(Padding::ZERO.right(12))
            .align_y(Alignment::Center),
    )
    .height(58)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| crate::style::outlined(ground, line, 8.0));
    let mut page = column![
        container(text("Add Friend").size(20).font(BOLD).color(theme.text))
            .height(24)
            .align_y(Alignment::Center),
        container(
            text("You can add friends with their Discord username.")
                .size(15)
                .font(SEMIBOLD)
                .color(theme.text)
        )
        .height(20)
        .align_y(Alignment::Center),
        Space::new().height(8),
        field,
    ]
    .spacing(7)
    .padding(Padding {
        top: 20.0,
        left: 30.0,
        right: 30.0,
        bottom: 0.0,
    });
    let (note, ink) = match dialog_state {
        AddFriend { sending: true, .. } => ("Sending...".to_owned(), theme.muted),
        AddFriend {
            outcome: Some(Ok(name)),
            ..
        } => (format!("Sent {name} a friend request."), theme.live),
        AddFriend {
            outcome: Some(Err(error)),
            ..
        } => (error.clone(), theme.danger),
        _ => (String::new(), theme.muted),
    };
    if !note.is_empty() {
        page = page.push(text(note).size(13).color(ink));
    }
    page.height(Length::Fill).into()
}

/// A friend in the list: ruled from the one above unless pointed at; a
/// click opens their DM.
fn friend_row<'a>(
    ctx: crate::ctx::Ctx<'a>,
    friend: &'a Friend,
    spot: Option<(&'a str, &'a str, &'a str)>,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let pointed = ctx.pointed.member.as_deref() == Some(friend.id.as_str());
    let surface = if pointed { theme.hover } else { theme.chat };
    let presence = friend.presence.unwrap_or(Presence::Offline);
    let face = Face::new(friend.avatar.as_deref(), &friend.name, 32.0)
        .decoration(
            ctx.state.cosmetics.decoration(friend.decoration.as_deref()),
            pointed,
        )
        .dot(Some((presence, rgb(surface), Point::new(20.0, 20.0))));
    let name = crate::elide::elide(friend.name.as_str())
        .size(16)
        .font(BOLD)
        .color(theme.text);
    let status = Status::of(friend.activity.as_deref());
    let sub: Element<'a, Message> = match (spot, status.is_empty()) {
        (Some(spot), true) => ctx.voice_spot(spot, 13.0, false),
        (spot, false) => {
            let mut line = row![].spacing(4).align_y(Alignment::Center);
            if let Some(spot) = spot {
                line = line.push(ctx.voice_spot(spot, 13.0, true));
            }
            line.push(ctx.status_line(
                status,
                StatusInk::words(13.0, iced::Font::default(), theme.muted),
            ))
            .into()
        }
        (None, true) => text(presence_word(presence))
            .size(13)
            .color(theme.muted)
            .into(),
    };
    let message = Message::Friends(Event::Message(friend.id.clone()));
    // Discord's buttons have no ground at rest, sit 10 px apart and 2 px
    // lower, the second the friend's menu behind a kebab.
    let round = |icon, label, message: Message| {
        let face = button(ctx.look.glyph(icon, 20, theme.secondary).boxed(36))
            .padding(0)
            .width(36)
            .height(36)
            .on_press(message)
            .style(lit_style(None, theme.active, 18.0));
        iced::widget::tooltip(
            face,
            controls::tip(theme, label),
            iced::widget::tooltip::Position::Bottom,
        )
        .gap(6)
    };
    let buttons = row![
        round(Icon::MessageCircle, "Message", message.clone()),
        round(
            Icon::EllipsisVertical,
            "More",
            Message::Friends(Event::Menu(friend.id.clone()))
        )
    ]
    .spacing(10);
    let content = row![
        ctx.person(Person::new(face, name).sub(sub)),
        container(buttons).padding(Padding::ZERO.top(2))
    ]
    .spacing(12)
    .padding(Padding::ZERO.right(20))
    .height(ROW)
    .align_y(Alignment::Center);
    let ground = theme.hover;
    let row_face = button(content)
        .padding(0)
        .width(Length::Fill)
        .on_press(message)
        .style(crate::controls::lit_style(None, ground, 8.0));
    // Discord rules the rows from 6 px in to 20 px short of the right.
    let line = if theme.bonfire {
        crate::style::hex(0x28282d)
    } else {
        theme.line
    };
    let mut layers = stack![row_face];
    if !pointed {
        layers = layers.push(
            container(controls::bar(line, Length::Fill, 1))
                .padding(Padding::ZERO.left(6).right(20)),
        );
    }
    let id = friend.id.clone();
    let area = mouse_area(layers)
        .on_enter(Message::Members(crate::members::Event::Point(
            friend.id.clone(),
            true,
        )))
        .on_exit(Message::Members(crate::members::Event::Point(
            friend.id.clone(),
            false,
        )));
    RightPress::new(area, move |_| Message::Friends(Event::Menu(id.clone()))).into()
}

/// The dialog that adds a Discord friend by username.
pub fn add_friend_view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let dialog_state = ctx.state.add_friend.as_ref()?;
    if add_page(ctx) {
        return None;
    }
    let theme = ctx.look.theme;
    let close = button(ctx.look.glyph(Icon::X, 18, theme.secondary).boxed(28))
        .padding(0)
        .on_press(Message::Friends(Event::CloseAdd))
        .style(lit_style(None, theme.hover, 6.0));
    let sending = dialog_state.sending;
    let name = &ctx.friends.add_name;
    let mut input = field(theme, ADD_FIELD, "Username", name, false, true)
        .size(15)
        .padding(10)
        .on_input(|text| Message::Friends(Event::AddName(text)));
    if !sending && !name.is_empty() {
        input = input.on_submit(Message::Friends(Event::Send));
    }
    let send = action(
        theme,
        "Send request",
        Kind::Primary,
        (!sending && !name.is_empty()).then_some(Message::Friends(Event::Send)),
    );
    let mut body = column![
        row![
            text("Add a friend")
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            close
        ]
        .align_y(Alignment::Center),
        text("Send someone a friend request on Discord with their username.")
            .size(14)
            .color(theme.secondary),
        row![input, send].spacing(8).align_y(Alignment::Center),
    ]
    .spacing(12)
    .padding(20);
    let (note, ink) = match dialog_state {
        AddFriend { sending: true, .. } => ("Sending...".to_owned(), theme.muted),
        AddFriend {
            outcome: Some(Ok(name)),
            ..
        } => (format!("Sent {name} a friend request."), theme.live),
        AddFriend {
            outcome: Some(Err(error)),
            ..
        } => (error.clone(), theme.danger),
        _ => (String::new(), theme.muted),
    };
    if !note.is_empty() {
        body = body.push(text(note).size(14).color(ink));
    }
    let width = ADD_WIDTH.min(ctx.look.window.width - 48.0);
    Some(dialog(
        theme,
        body.into(),
        width,
        Message::Friends(Event::CloseAdd),
    ))
}

/// The friends shown: those around, or all, whose names hold the
/// search's text.
fn friends_shown<'a>(ctx: crate::ctx::Ctx<'a>) -> Vec<&'a Friend> {
    let query = ctx.friends.filter.trim().to_lowercase();
    ctx.state
        .friends
        .iter()
        .filter(|friend| ctx.friends.all || around(friend))
        .filter(|friend| query.is_empty() || friend.name.to_lowercase().contains(&query))
        .collect()
}
