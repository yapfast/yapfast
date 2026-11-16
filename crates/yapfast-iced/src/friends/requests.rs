//! The Requests screen: friend requests and message requests, a person's
//! on one row with their latest message, then those sent, and spam folded
//! away, as the Slint client's `request_rows` lays them out.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, mouse_area, row, text},
};
use yapfast_core::{chat::ChatRoom, discord};

use crate::{
    app::Message,
    controls::{self, Kind, action},
    fonts::{BOLD, MEDIUM},
    pictures::Icon,
    style::{rounded, scrollbar},
    theme::rgb,
};

use super::Event;

/// How many wait for this account's answer, as the Requests row counts
/// them: a person's friend request and message request once.
pub fn waiting(state: &yapfast_core::application::AppState) -> usize {
    request_rows(state).incoming.len()
}

/// Someone on the requests screen.
pub(super) struct Request<'a> {
    /// Their Discord user id, or a group's room id.
    pub(super) id: &'a str,
    pub(super) name: &'a str,
    pub(super) avatar: Option<&'a str>,
    /// A friend request: sent to this account (true) or by it.
    pub(super) friend: Option<bool>,
    /// Their message request's room, its latest message and when it came.
    pub(super) room: Option<&'a str>,
    pub(super) message: Option<&'a str>,
    pub(super) time: String,
    pub(super) unread: bool,
}

/// The requests screen's rows.
pub(super) struct RequestRows<'a> {
    pub(super) incoming: Vec<Request<'a>>,
    pub(super) sent: Vec<Request<'a>>,
    pub(super) spam: Vec<Request<'a>>,
}

/// Friend requests and message requests as the requests screen lists
/// them, someone's friend request and message request on one row, as the
/// Slint client's `request_rows`.
pub(super) fn request_rows(state: &yapfast_core::application::AppState) -> RequestRows<'_> {
    let waiting = |name: &str| {
        state
            .scoped_rooms()
            .filter(|room| room.category == name)
            .collect::<Vec<_>>()
    };
    let (message_requests, spam) = (waiting(discord::REQUESTS), waiting(discord::SPAM));
    let mut rows = RequestRows {
        incoming: Vec::new(),
        sent: Vec::new(),
        spam: Vec::new(),
    };
    let with_room = |row: &mut Request<'_>, room: &ChatRoom| {
        row.unread = room.unread > 0;
        if let Some(preview) = &room.preview {
            let seconds = preview.at / 1000;
            let today = yapfast_core::clock::date(yapfast_core::clock::now_ms() / 1000);
            row.time = if yapfast_core::clock::date(seconds) == today {
                yapfast_core::clock::time(seconds)
            } else {
                yapfast_core::clock::day(seconds)
            };
        }
    };
    let rooms = || message_requests.iter().chain(&spam);
    for request in &state.friend_requests {
        let room = rooms().find(|room| room.person.as_deref() == Some(request.id.as_str()));
        let mut row = Request {
            id: &request.id,
            name: &request.name,
            avatar: request.avatar.as_deref(),
            friend: Some(request.incoming),
            room: room.map(|room| room.id.as_str()),
            message: room
                .and_then(|room| room.preview.as_ref().map(|preview| preview.text.as_str())),
            time: String::new(),
            unread: false,
        };
        if let Some(room) = room {
            with_room(&mut row, room);
        }
        let spammed = request.spam || room.is_some_and(|room| room.category == discord::SPAM);
        if spammed {
            rows.spam.push(row);
        } else if request.incoming || room.is_some() {
            // A message waits for an answer, whoever asked to be friends.
            rows.incoming.push(row);
        } else {
            rows.sent.push(row);
        }
    }
    let befriending = |room: &&&ChatRoom| {
        !state
            .friend_requests
            .iter()
            .any(|request| room.person.as_deref() == Some(request.id.as_str()))
    };
    for (list, rooms) in [
        (&mut rows.incoming, &message_requests),
        (&mut rows.spam, &spam),
    ] {
        for room in rooms.iter().filter(befriending) {
            let mut row = Request {
                id: room.person.as_deref().unwrap_or(&room.id),
                name: &room.name,
                avatar: room.avatar.as_deref(),
                friend: None,
                room: Some(&room.id),
                message: room.preview.as_ref().map(|preview| preview.text.as_str()),
                time: String::new(),
                unread: false,
            };
            with_room(&mut row, room);
            list.push(row);
        }
    }
    rows
}

impl<'a> crate::ctx::Ctx<'a> {
    /// Friend requests and message requests, a person's together, then
    /// those this account sent, and spam folded away.
    pub fn requests_screen(self) -> Element<'a, Message> {
        let state = self.state;
        let rows = request_rows(state);
        if rows.incoming.is_empty() && rows.sent.is_empty() && rows.spam.is_empty() {
            return self.look.empty_screen(
                Icon::Inbox,
                "No requests",
                "Friend requests and messages from people you don't know show up here.",
                None,
            );
        }
        let mut list = column![].spacing(2).padding(Padding {
            top: 8.0,
            left: 16.0,
            right: 16.0,
            bottom: 16.0,
        });
        if !rows.incoming.is_empty() {
            list = list.push(self.list_heading(format!("REQUESTS — {}", rows.incoming.len())));
        }
        for request in rows.incoming {
            list = list.push(self.request_item(request, false));
        }
        if !rows.sent.is_empty() {
            list = list.push(self.list_heading(format!("SENT — {}", rows.sent.len())));
        }
        for request in rows.sent {
            list = list.push(self.request_item(request, false));
        }
        if !rows.spam.is_empty() {
            let theme = self.look.theme;
            let open = self.friends.spam_open;
            let (muted, secondary) = (theme.muted, theme.secondary);
            let chevron = if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            list = list.push(
                button(
                    row![
                        text(format!("SPAM — {}", rows.spam.len()))
                            .size(12)
                            .font(BOLD),
                        self.look.glyph(chevron, 14, muted),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding([0, 8])
                .height(36)
                .on_press(Message::Friends(Event::Spam))
                .style(move |_, status| button::Style {
                    text_color: if controls::lights(status) {
                        secondary
                    } else {
                        muted
                    },
                    ..Default::default()
                }),
            );
            if open {
                for request in rows.spam {
                    list = list.push(self.request_item(request, true));
                }
            }
        }
        scroll_view(list)
            .height(Length::Fill)
            .style(scrollbar(self.look.theme))
            .into()
    }

    /// Someone waiting for an answer, as the Slint client's `RequestItem`:
    /// their friend request, answered here, and their message request's
    /// latest message under it, which opens the conversation.
    pub(super) fn request_item(self, request: Request<'a>, spam: bool) -> Element<'a, Message> {
        let theme = self.look.theme;
        let messaged = request.room.is_some();
        let about = match (spam, request.friend, messaged) {
            (true, _, _) => "Maybe spam",
            (_, Some(true), true) => "Sent you a friend request and a message",
            (_, Some(true), false) => "Sent you a friend request",
            (_, Some(false), _) => "You sent a friend request",
            (_, None, _) => "Wants to message you",
        };
        let bold = request.unread || request.friend == Some(true);
        let mut words = column![
            crate::elide::elide(request.name)
                .size(16)
                .font(if bold { BOLD } else { MEDIUM })
                .color(theme.text),
            text(about)
                .size(13)
                .color(if spam { theme.loud } else { theme.muted })
                .wrapping(text::Wrapping::None),
        ]
        .spacing(1)
        .width(Length::Fill);
        words = words.padding([2, 0]);
        let mut top = row![words].spacing(8).align_y(Alignment::Center);
        let id = request.id.to_owned();
        if request.friend == Some(true) {
            let check = self.look.icon(Icon::Check, 16, rgb(theme.on_accent));
            top = top.push(
                action(
                    theme,
                    "Accept",
                    Kind::Primary,
                    Some(Message::Friends(Event::Accept(id.clone()))),
                )
                .icon(check)
                .small(),
            );
        }
        if let Some(incoming) = request.friend {
            let x = self.look.icon(Icon::X, 16, rgb(theme.text));
            top = top.push(
                action(
                    theme,
                    if incoming { "Ignore" } else { "Cancel" },
                    Kind::Plain,
                    Some(Message::Friends(Event::Remove(id))),
                )
                .icon(x)
                .small(),
            );
        }
        let mut body = column![top].spacing(8).width(Length::Fill);
        if let Some(room) = request.room {
            body = body.push(self.request_message(&request, room));
        }
        // Slint's Discord badge on the avatar shows only with its
        // setting, which this client doesn't have yet.
        let avatar = self.avatar(request.avatar, request.name, 40.0, 20.0, 1.0);
        let pointed = self.pointed.member.as_deref() == Some(request.id);
        let ground = if pointed {
            theme.row_hover
        } else {
            Color::TRANSPARENT
        };
        let item = container(
            row![avatar, body]
                .spacing(12)
                .padding(10)
                .align_y(Alignment::Start),
        )
        .style(move |_| rounded(ground, 10))
        .width(Length::Fill);
        mouse_area(item)
            .on_enter(Message::PointMember(request.id.to_owned(), true))
            .on_exit(Message::PointMember(request.id.to_owned(), false))
            .into()
    }

    /// A message request's latest message, under who sent it; a click
    /// opens the conversation.
    pub(super) fn request_message(
        self,
        request: &Request<'a>,
        room: &'a str,
    ) -> Element<'a, Message> {
        let theme = self.look.theme;
        let unread = request.unread;
        let (words, ink) = match request.message {
            Some(message) if !message.is_empty() => {
                (message, if unread { theme.text } else { theme.secondary })
            }
            _ => ("Open to read their message", theme.muted),
        };
        let mut end = row![].spacing(6).align_y(Alignment::Center);
        if unread {
            let (accent, on_accent) = (theme.accent, theme.on_accent);
            end = end.push(
                container(text("New").size(11).font(BOLD).color(on_accent))
                    .padding([0, 5])
                    .height(16)
                    .align_y(Alignment::Center)
                    .style(move |_| rounded(accent, 4)),
            );
        }
        if !request.time.is_empty() {
            end = end.push(text(request.time.clone()).size(12).color(theme.muted));
        }
        end = end.push(self.look.glyph(Icon::ChevronRight, 16, theme.muted));
        let accent = theme.accent;
        let mark: Element<'a, Message> = if unread {
            container(Space::new())
                .width(3)
                .height(Length::Fill)
                .style(move |_| rounded(accent, crate::style::corners([8.0, 0.0, 0.0, 8.0])))
                .into()
        } else {
            Space::new().width(0).into()
        };
        let content = row![
            container(self.look.glyph(
                Icon::MessageCircle,
                16,
                if unread { theme.accent } else { theme.muted }
            ))
            .padding(Padding {
                top: 2.0,
                ..Padding::ZERO
            }),
            container(text(words).size(14).color(ink))
                .max_height(40)
                .clip(true)
                .width(Length::Fill),
            end,
        ]
        .spacing(10)
        .padding(Padding {
            top: 10.0,
            bottom: 10.0,
            left: 12.0,
            right: 10.0,
        });
        let (raised, active) = (theme.raised, theme.active);
        button(row![mark, content].height(Length::Shrink))
            .padding(0)
            .width(Length::Fill)
            .on_press(Message::Friends(Event::Open(room.to_owned())))
            .style(move |_, status| button::Style {
                background: Some(
                    if controls::lights(status) {
                        active
                    } else {
                        raised
                    }
                    .into(),
                ),
                border: Border::default().rounded(8),
                ..Default::default()
            })
            .into()
    }
}
