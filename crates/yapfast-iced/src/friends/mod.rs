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
    fonts::{BOLD, MEDIUM},
    looks::Status,
    people::StatusInk,
    people::{Person, voice_spots},
    pictures::{Face, Icon},
    style::scrollbar,
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
            urls.extend(
                shown[first..last]
                    .iter()
                    .filter_map(|friend| friend.avatar.clone()),
            );
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

/// The header over the friends list or the requests: the screen's icon
/// and name, and on the friends list its tabs and Add Friend.
pub fn friends_header<'a>(ctx: crate::ctx::Ctx<'a>, screen: DmScreen) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (icon, name) = match screen {
        DmScreen::Friends => (Icon::Users, "Friends"),
        DmScreen::Requests => (Icon::Inbox, "Requests"),
    };
    let mut head = row![
        ctx.look.glyph(icon, 22, theme.muted),
        text(name).size(16).font(BOLD).color(theme.text)
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if screen == DmScreen::Friends {
        let all = ctx.friends.all;
        head = head
            .push(controls::bar(theme.line, 1, 24))
            .push(header_tab(ctx, "Online", !all, Event::All(false)))
            .push(header_tab(ctx, "All", all, Event::All(true)))
            .push(
                action(
                    theme,
                    "Add Friend",
                    Kind::Primary,
                    Some(Message::Friends(Event::OpenAdd)),
                )
                .small(),
            );
    }
    container(head.push(Space::new().width(Length::Fill)))
        .padding([0, 16])
        .width(Length::Fill)
        .height(crate::header::HEIGHT)
        .align_y(Alignment::Center)
        .into()
}

/// A tab in the friends header.
fn header_tab<'a>(
    ctx: crate::ctx::Ctx<'a>,
    label: &'static str,
    chosen: bool,
    event: Event,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (text_ink, secondary, active, hover) =
        (theme.text, theme.secondary, theme.active, theme.hover);
    button(
        container(text(label).size(15).font(MEDIUM))
            .padding([0, 10])
            .height(28)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press(Message::Friends(event))
    .style(move |_, status| {
        let lit = controls::lights(status);
        button::Style {
            background: if chosen {
                Some(active.into())
            } else if lit {
                Some(hover.into())
            } else {
                None
            },
            text_color: if chosen || lit { text_ink } else { secondary },
            border: crate::style::round(6),
            ..Default::default()
        }
    })
    .into()
}

/// The friends list, beside Active Now when there's room for it.
pub fn friends_screen<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let wide = crate::active_now::active_now_fits(ctx);
    let list = friends_list(ctx);
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
    let search = field(theme, SEARCH, "Search", &ctx.friends.filter, false, true)
        .size(15)
        .padding([8, 10])
        .on_input(|text| Message::Friends(Event::Filter(text)));
    let heading = format!(
        "{} — {}",
        if ctx.friends.all {
            "ALL FRIENDS"
        } else {
            "ONLINE"
        },
        shown.len()
    );
    let mut page = column![search, ctx.list_heading(heading)]
        .spacing(8)
        .padding(Padding {
            top: 16.0,
            left: 16.0,
            right: 16.0,
            bottom: 0.0,
        })
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
            friend_row(ctx, friend, spots.get(friend.id.as_str()).copied()),
        );
    }
    let list = scroll_view(rows)
        .id(iced::widget::Id::new(LIST))
        .on_scroll(|viewport| Message::Friends(Event::Scrolled(Scroll::of(viewport))))
        .height(Length::Fill)
        .style(scrollbar(theme));
    page.push(list).into()
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
    let face = Face::new(friend.avatar.as_deref(), &friend.name, 32.0).dot(Some((
        presence,
        rgb(surface),
        Point::new(20.0, 20.0),
    )));
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
    let (raised, hover) = (theme.raised, theme.active);
    let send = button(
        ctx.look
            .glyph(Icon::MessageCircle, 20, theme.secondary)
            .boxed(36),
    )
    .padding(0)
    .width(36)
    .height(36)
    .on_press(message.clone())
    .style(lit_style(Some(raised), hover, 18.0));
    let send = iced::widget::tooltip(
        send,
        controls::tip(theme, "Message"),
        iced::widget::tooltip::Position::Bottom,
    )
    .gap(6);
    let content = row![ctx.person(Person::new(face, name).sub(sub)), send]
        .spacing(12)
        .padding([0, 10])
        .height(ROW)
        .align_y(Alignment::Center);
    let ground = theme.hover;
    let row_face = button(content)
        .padding(0)
        .width(Length::Fill)
        .on_press(message)
        .style(crate::controls::lit_style(None, ground, 8.0));
    let line = theme.line;
    let mut layers = stack![row_face];
    if !pointed {
        layers = layers.push(container(controls::bar(line, Length::Fill, 1)).padding([0, 10]));
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
