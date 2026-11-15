//! The Discord friends list and the requests screen, in place of a
//! conversation on the DM home, as the Slint client's `FriendsScreen` and
//! `RequestsScreen`: the header's Online and All tabs and Add Friend; the
//! friends with their presence, voice channel and status, a message button
//! and their menu; friend and message requests with their answers, those
//! sent, and spam folded away; and the dialog that adds a friend by
//! username.

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Task,
    widget::{
        Space, button, column, container, mouse_area, operation, row, scrollable, stack, text,
    },
};
use yapfast_core::{
    application::{Action, AddFriend, DmScreen, Navigation},
    chat::{ChatRoom, Friend, Presence},
    discord,
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action, field, lit_style},
    dialogs::dialog,
    fonts::{BOLD, MEDIUM},
    looks::Status,
    people::{Person, voice_spots},
    pictures::{Face, Icon},
    style::{rounded, scrollbar},
    sync::Scroll,
    theme::rgb,
    widgets::{RightPress, Rows},
};

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

impl App {
    /// The DM home's screen in place of a conversation, if one shows.
    pub fn dm_screen(&self) -> Option<DmScreen> {
        let state = self.client.state();
        (state.navigation == Navigation::DirectMessages && state.selected.is_none())
            .then_some(state.dm_screen)
            .flatten()
    }

    pub fn friends_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::All(all) => {
                self.friends.all = all;
                self.friends.scroll = None;
                return Task::batch([
                    operation::scroll_to(
                        LIST,
                        iced::widget::scrollable::AbsoluteOffset {
                            x: None,
                            y: Some(0.0),
                        },
                    ),
                    self.ask(),
                ]);
            }
            Event::Filter(filter) => {
                self.friends.filter = filter;
                return self.ask();
            }
            Event::Scrolled(scroll) => {
                self.friends.scroll = Some(scroll);
                return self.ask();
            }
            Event::Message(user) => self.client.dispatch(Action::MessagePerson(user)),
            Event::Menu(user) => {
                let at = self.pressed.get();
                return self.open_person_menu(&user, at, crate::person::CONTEXT_MENU);
            }
            Event::Accept(user) => self.client.dispatch(Action::Befriend(user)),
            Event::Remove(user) => self.client.dispatch(Action::RemoveFriendRequest(user)),
            Event::Open(room) => {
                return self.update(Message::Side(crate::sidebar::Event::Select(room)));
            }
            Event::Spam => {
                self.friends.spam_open = !self.friends.spam_open;
                return self.ask();
            }
            Event::OpenAdd => {
                self.client.dispatch(Action::OpenAddFriend);
                self.friends.add_name.clear();
                self.friends.add_sent = false;
                return Task::batch([self.sync(), operation::focus(ADD_FIELD)]);
            }
            Event::AddName(name) => {
                self.friends.add_name = name;
                return Task::none();
            }
            Event::Send => {
                let name = self.friends.add_name.clone();
                if !name.trim().is_empty() {
                    self.client.dispatch(Action::AddFriend(name));
                }
            }
            Event::CloseAdd => {
                self.client.dispatch(Action::CloseAddFriend);
                return Task::batch([self.sync(), self.refocus()]);
            }
        }
        self.sync()
    }

    /// A request that went clears the name, ready for the next.
    pub fn friends_sync(&mut self) {
        let sent = self
            .client
            .state()
            .add_friend
            .as_ref()
            .is_some_and(|dialog| matches!(dialog.outcome, Some(Ok(_))));
        if sent && !self.friends.add_sent {
            self.friends.add_name.clear();
        }
        self.friends.add_sent = sent;
    }

    /// The header over the friends list or the requests: the screen's icon
    /// and name, and on the friends list its tabs and Add Friend.
    pub fn friends_header(&self, screen: DmScreen) -> Element<'_, Message> {
        let theme = &self.theme;
        let (icon, name) = match screen {
            DmScreen::Friends => (Icon::Users, "Friends"),
            DmScreen::Requests => (Icon::Inbox, "Requests"),
        };
        let mut head = row![
            self.glyph(icon, 22, theme.muted),
            text(name).size(16).font(BOLD).color(theme.text)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if screen == DmScreen::Friends {
            let all = self.friends.all;
            head = head
                .push(controls::bar(theme.line, 1, 24))
                .push(self.header_tab("Online", !all, Event::All(false)))
                .push(self.header_tab("All", all, Event::All(true)))
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

    /// A tab in the friends header, as the Slint client's `HeaderTab`.
    fn header_tab(&self, label: &'static str, chosen: bool, event: Event) -> Element<'_, Message> {
        let theme = &self.theme;
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
                border: Border::default().rounded(6),
                ..Default::default()
            }
        })
        .into()
    }

    /// The friends shown: those around, or all, whose names hold the
    /// search's text.
    fn friends_shown(&self) -> Vec<&Friend> {
        let query = self.friends.filter.trim().to_lowercase();
        self.client
            .state()
            .friends
            .iter()
            .filter(|friend| self.friends.all || around(friend))
            .filter(|friend| query.is_empty() || friend.name.to_lowercase().contains(&query))
            .collect()
    }

    /// The friends list, beside Active Now when there's room for it.
    pub fn friends_screen(&self, width: f32) -> Element<'_, Message> {
        let wide = width >= crate::active_now::ROOM;
        let list = self.friends_list();
        if !wide {
            return list;
        }
        row![
            container(list).width(Length::Fill).clip(true),
            self.active_now_view()
        ]
        .into()
    }

    fn friends_list(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        if state.friends.is_empty() {
            return self.empty_screen(
                Icon::Users,
                "No friends yet",
                "Add someone with their Discord username.",
                Some(("Add a friend", Message::Friends(Event::OpenAdd))),
            );
        }
        let shown = self.friends_shown();
        let search = field(theme, SEARCH, "Search", &self.friends.filter, false, true)
            .size(15)
            .padding([8, 10])
            .on_input(|text| Message::Friends(Event::Filter(text)));
        let heading = format!(
            "{} — {}",
            if self.friends.all {
                "ALL FRIENDS"
            } else {
                "ONLINE"
            },
            shown.len()
        );
        let mut page = column![search, self.list_heading(heading)]
            .spacing(8)
            .padding(Padding {
                top: 16.0,
                left: 16.0,
                right: 16.0,
                bottom: 0.0,
            })
            .height(Length::Fill);
        if shown.is_empty() {
            let words = if self.friends.filter.is_empty() {
                "None of your friends are around right now."
            } else {
                "No friend's name matches that."
            };
            page =
                page.push(container(text(words).size(14).color(theme.secondary)).padding([0, 8]));
        }
        let scroll = self
            .friends
            .scroll
            .unwrap_or(Scroll::new(self.window.height));
        let (first, last) = scroll.range(shown.len(), ROW, 0.0);
        let total = shown.len() as f32 * ROW + 16.0;
        let spots = voice_spots(state);
        let mut rows = Rows::new(total);
        for (index, friend) in shown.iter().enumerate().take(last).skip(first) {
            rows.push(
                crate::app::key(&friend.id),
                index as f32 * ROW,
                ROW,
                self.friend_row(friend, spots.get(friend.id.as_str()).copied()),
            );
        }
        let list = scrollable(rows)
            .id(iced::widget::Id::new(LIST))
            .on_scroll(|viewport| Message::Friends(Event::Scrolled(Scroll::of(viewport))))
            .height(Length::Fill)
            .style(scrollbar(theme));
        page.push(list).into()
    }

    /// A friend in the list, as the Slint client's `FriendItem`: ruled from
    /// the one above unless pointed at; a click opens their DM.
    fn friend_row<'a>(
        &'a self,
        friend: &'a Friend,
        spot: Option<(&'a str, &'a str, &'a str)>,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let pointed = self.pointed_member.as_deref() == Some(friend.id.as_str());
        let surface = if pointed { theme.hover } else { theme.chat };
        let presence = friend.presence.unwrap_or(Presence::Offline);
        let face = Face {
            url: friend.avatar.as_deref(),
            name: &friend.name,
            key: None,
            size: 32.0,
            opacity: 1.0,
            decoration: None,
            play: false,
            dot: Some((presence, rgb(surface), Point::new(20.0, 20.0))),
        };
        let name = text(friend.name.as_str())
            .size(16)
            .font(BOLD)
            .color(theme.text)
            .wrapping(text::Wrapping::None);
        let status = Status::of(friend.activity.as_deref());
        let sub: Element<'a, Message> = match (spot, status.is_empty()) {
            (Some(spot), true) => self.voice_spot(spot, 13.0, false),
            (spot, false) => {
                let mut line = row![].spacing(4).align_y(Alignment::Center);
                if let Some(spot) = spot {
                    line = line.push(self.voice_spot(spot, 13.0, true));
                }
                line.push(self.status_text(status, 13.0, iced::Font::default(), theme.muted))
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
            self.glyph(Icon::MessageCircle, 20, theme.secondary)
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
        let content = row![self.person(Person::new(face, name).sub(sub)), send]
            .spacing(12)
            .padding([0, 10])
            .height(ROW)
            .align_y(Alignment::Center);
        let ground = theme.hover;
        let row_face = button(content)
            .padding(0)
            .width(Length::Fill)
            .on_press(message)
            .style(move |_, status| button::Style {
                background: controls::lights(status).then_some(ground.into()),
                border: Border::default().rounded(8),
                ..Default::default()
            });
        let line = theme.line;
        let mut layers = stack![row_face];
        if !pointed {
            layers = layers.push(container(controls::bar(line, Length::Fill, 1)).padding([0, 10]));
        }
        let id = friend.id.clone();
        let area = mouse_area(layers)
            .on_enter(Message::PointMember(friend.id.clone(), true))
            .on_exit(Message::PointMember(friend.id.clone(), false));
        RightPress::new(area, move |_| Message::Friends(Event::Menu(id.clone()))).into()
    }

    /// An empty screen's words, under its icon on a disc, and maybe a
    /// button.
    fn empty_screen<'a>(
        &self,
        icon: Icon,
        heading: &'a str,
        words: &'a str,
        offer: Option<(&'a str, Message)>,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let raised = theme.raised;
        let mut page = column![
            container(self.glyph(icon, 36, theme.secondary))
                .center(80)
                .style(move |_| rounded(raised, 40)),
            text(heading).size(22).font(BOLD).color(theme.text),
            text(words)
                .size(14)
                .color(theme.secondary)
                .align_x(Alignment::Center),
        ]
        .spacing(10)
        .align_x(Alignment::Center);
        if let Some((label, message)) = offer {
            page = page.push(action(theme, label, Kind::Primary, Some(message)));
        }
        container(page).padding(32).center(Length::Fill).into()
    }

    /// Friend requests and message requests, a person's together, then
    /// those this account sent, and spam folded away.
    pub fn requests_screen(&self) -> Element<'_, Message> {
        let state = self.client.state();
        let rows = request_rows(state);
        if rows.incoming.is_empty() && rows.sent.is_empty() && rows.spam.is_empty() {
            return self.empty_screen(
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
            let theme = &self.theme;
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
                        self.glyph(chevron, 14, muted),
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
        scrollable(list)
            .height(Length::Fill)
            .style(scrollbar(&self.theme))
            .into()
    }

    /// Someone waiting for an answer, as the Slint client's `RequestItem`:
    /// their friend request, answered here, and their message request's
    /// latest message under it, which opens the conversation.
    fn request_item<'a>(&'a self, request: Request<'a>, spam: bool) -> Element<'a, Message> {
        let theme = &self.theme;
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
            text(request.name)
                .size(16)
                .font(if bold { BOLD } else { MEDIUM })
                .color(theme.text)
                .wrapping(text::Wrapping::None),
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
            let check = self.icon(Icon::Check, 16, rgb(theme.on_accent));
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
            let x = self.icon(Icon::X, 16, rgb(theme.text));
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
        let face = self.avatar(request.avatar, request.name, 40.0, 20.0, 1.0);
        // Slint's Discord badge on the avatar shows only with its
        // setting, which this client doesn't have yet.
        let avatar = face;
        let pointed = self.pointed_member.as_deref() == Some(request.id);
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
    fn request_message<'a>(&'a self, request: &Request<'a>, room: &'a str) -> Element<'a, Message> {
        let theme = &self.theme;
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
        end = end.push(self.glyph(Icon::ChevronRight, 16, theme.muted));
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
            container(self.glyph(
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

    /// The dialog that adds a Discord friend by username.
    pub fn add_friend_view(&self) -> Option<Element<'_, Message>> {
        let dialog_state = self.client.state().add_friend.as_ref()?;
        let theme = &self.theme;
        let close = button(self.glyph(Icon::X, 18, theme.secondary).boxed(28))
            .padding(0)
            .on_press(Message::Friends(Event::CloseAdd))
            .style(lit_style(None, theme.hover, 6.0));
        let sending = dialog_state.sending;
        let name = &self.friends.add_name;
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
        let width = ADD_WIDTH.min(self.window.width - 48.0);
        Some(dialog(
            theme,
            body.into(),
            width,
            Message::Friends(Event::CloseAdd),
        ))
    }

    /// The pictures the friends and requests screens show: the avatars of
    /// the friends in view, the requests', and Active Now's.
    pub fn friends_pictures(&self) -> Vec<String> {
        let mut urls = Vec::new();
        match self.dm_screen() {
            Some(DmScreen::Friends) => {
                let shown = self.friends_shown();
                let scroll = self
                    .friends
                    .scroll
                    .unwrap_or(Scroll::new(self.window.height));
                let (first, last) = scroll.range(shown.len(), ROW, 0.0);
                urls.extend(
                    shown[first..last]
                        .iter()
                        .filter_map(|friend| friend.avatar.clone()),
                );
                urls.extend(self.active_now_pictures());
            }
            Some(DmScreen::Requests) => {
                let rows = request_rows(self.client.state());
                for request in rows.incoming.iter().chain(&rows.sent).chain(&rows.spam) {
                    urls.extend(request.avatar.map(str::to_owned));
                }
            }
            None => {}
        }
        urls
    }
}

/// How many wait for this account's answer, as the Requests row counts
/// them: a person's friend request and message request once.
pub fn waiting(state: &yapfast_core::application::AppState) -> usize {
    request_rows(state).incoming.len()
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

/// Someone on the requests screen.
struct Request<'a> {
    /// Their Discord user id, or a group's room id.
    id: &'a str,
    name: &'a str,
    avatar: Option<&'a str>,
    /// A friend request: sent to this account (true) or by it.
    friend: Option<bool>,
    /// Their message request's room, its latest message and when it came.
    room: Option<&'a str>,
    message: Option<&'a str>,
    time: String,
    unread: bool,
}

/// The requests screen's rows.
struct RequestRows<'a> {
    incoming: Vec<Request<'a>>,
    sent: Vec<Request<'a>>,
    spam: Vec<Request<'a>>,
}

/// Friend requests and message requests as the requests screen lists
/// them, someone's friend request and message request on one row, as the
/// Slint client's `request_rows`.
fn request_rows(state: &yapfast_core::application::AppState) -> RequestRows<'_> {
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
