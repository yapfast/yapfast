//! Cards that open over the window at what was clicked, as the Slint
//! client's: a role mentioned in a message (`RoleCard`, who Yapfast knows
//! has it, a click on one opening their card over it), and a mutual
//! server a profile lists (`ServerCard`, what it is and Go to server), and
//! an emoji in a message's text or a message's sticker (`EmojiCardView`,
//! `MessageStickerCardView`: what it is and the server it is from, as
//! `yapfast_core::emoji::card` works it out). Each closes on Escape or a
//! click outside; what they show is the core's.

use iced::{
    Alignment, Border, Element, Length, Padding, Point, Rectangle, Size, Task,
    border::Radius,
    widget::{button, column, container, opaque, row, scrollable, stack, text},
};
use yapfast_core::{
    application::{Action, Navigation},
    emoji::card::{self as expression, Target},
};

use crate::{
    app::{App, Message},
    controls::{self, Kind},
    elide::elide,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    media::{Fit, Shot},
    style::{raised_shadow, rounded, scrollbar},
    theme::Theme,
    widgets::Place,
};

/// The role card's width, and the most its list of people takes before
/// it scrolls.
const ROLE_WIDTH: f32 = 260.0;
const ROLE_LIST: f32 = 320.0;
/// A person's row on it, and their avatar.
const ROLE_ROW: f32 = 40.0;
const ROLE_AVATAR: f32 = 28.0;
/// The server card's width, its banner's height, and its icon's.
const SERVER_WIDTH: f32 = 300.0;
const SERVER_BANNER: f32 = 90.0;
const SERVER_ICON: f32 = 64.0;
/// The ring round the server's icon, of the card's ground.
const SERVER_RING: f32 = 6.0;
/// An emoji's or sticker's card: its width, the picture's side, the room
/// beside it for the name and description, and the server's icon.
const EXPRESSION_WIDTH: f32 = 288.0;
const EXPRESSION_PICTURE: f32 = 48.0;
const EXPRESSION_WORDS: f32 = 198.0;
const EXPRESSION_ICON: f32 = 32.0;
/// A Unicode emoji Twemoji has no picture of, drawn as text.
const EXPRESSION_GLYPH: f32 = 40.0;

/// What an emoji's or sticker's card is of.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Emoji(Target),
    /// A message's attachment, by the message's item and its index.
    Sticker(String, usize),
}

/// What's open, and where it was clicked.
#[derive(Default)]
pub struct Cards {
    /// A role card's place.
    pub role_at: Option<Point>,
    /// A mutual server's card: the server's id and its place.
    pub server: Option<(String, Point)>,
    /// An emoji's or sticker's card, and where in the window what was
    /// clicked is: it opens under it.
    pub expression: Option<(Expression, Rectangle)>,
}

#[derive(Clone, Debug)]
pub enum Event {
    CloseRole,
    /// Someone on the role card, by id: their card, at the press.
    RoleMember(String),
    /// A mutual server in a profile, by id: its card, at the press.
    Server(String),
    CloseServer,
    GoToServer(String),
    /// A message's sticker, by its item and attachment index: its card,
    /// under it.
    Sticker(String, usize),
    CloseExpression,
    /// The server an emoji's or sticker's card names.
    OpenServer(String),
}

/// The frame of a card over the window (the Slint client's `raised`
/// ground, edge and shadow), `width` wide, its corners `radius` round.
pub fn frame<'a>(
    theme: &Theme,
    ground: iced::Color,
    width: f32,
    radius: f32,
    child: impl Into<Element<'a, Message>>,
) -> container::Container<'a, Message> {
    let edge = theme.edge;
    container(child)
        .width(width)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border: crate::style::edge(edge, radius),
            shadow: raised_shadow(),
            ..Default::default()
        })
}

impl App {
    pub fn info_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::CloseRole => {
                self.cards.role_at = None;
                self.client.dispatch(Action::CloseRole);
            }
            Event::RoleMember(user) => {
                let at = self.pressed.get();
                return self.open_card(user, Point::new(at.x + 8.0, at.y));
            }
            Event::Server(server) => {
                self.cards.server = Some((server, self.pressed.get()));
                return self.ask();
            }
            Event::CloseServer => self.cards.server = None,
            Event::Sticker(item, index) => {
                return self.show_expression(Expression::Sticker(item, index));
            }
            Event::CloseExpression => self.cards.expression = None,
            Event::OpenServer(server) => {
                self.cards.expression = None;
                self.client
                    .dispatch(Action::Navigate(Navigation::Server(server)));
            }
            Event::GoToServer(server) => {
                self.cards.server = None;
                self.client.dispatch(Action::CloseProfile);
                self.card = None;
                self.client
                    .dispatch(Action::Navigate(Navigation::Server(server)));
            }
        }
        self.sync()
    }

    /// A role mention clicked at `at`: its card there.
    pub fn show_role(&mut self, role: String, at: Point) -> Task<Message> {
        self.client.dispatch(Action::ShowRole(role));
        self.cards.role_at = Some(Point::new(at.x + 8.0, at.y - 8.0));
        Task::batch([
            self.sync(),
            iced::widget::operation::focus(crate::app::NOBODY),
        ])
    }

    /// An emoji's or sticker's card, under what was pressed
    /// (`App::link_spot`, from the press), as the Slint client's: its server's card asks
    /// Discord how many are online there, and shows it when it answers.
    pub fn show_expression(&mut self, expression: Expression) -> Task<Message> {
        let card = self.expression_card(&expression);
        if let Some(server) = card.and_then(|card| card.server) {
            self.client.dispatch(Action::PreviewServer(server.id));
        }
        self.cards.expression = Some((expression, self.pressed_spot(self.link_spot.get())));
        Task::batch([
            self.sync(),
            iced::widget::operation::focus(crate::app::NOBODY),
        ])
    }

    /// A place noted from the press (`widgets::Spot::by_press`, a link's
    /// in text), in the window.
    pub fn pressed_spot(&self, from_press: Rectangle) -> Rectangle {
        from_press + (self.pressed.get() - Point::ORIGIN)
    }

    /// What the open emoji's or sticker's card says, from the core.
    /// Lets go of the cards whose subject went (a sticker's message
    /// deleted, the profile listing a mutual server closed): drawn as
    /// nothing, they'd still hold the keys and take the next Escape.
    pub fn info_sync(&mut self) {
        if let Some((expression, _)) = &self.cards.expression
            && self.expression_card(expression).is_none()
        {
            self.cards.expression = None;
        }
        if self.cards.server.is_some() && self.card_server().is_none() {
            self.cards.server = None;
        }
    }

    fn expression_card(&self, expression: &Expression) -> Option<expression::Card> {
        match expression {
            Expression::Emoji(target) => Some(expression::emoji_for(&self.client.app, target)),
            Expression::Sticker(item, index) => {
                expression::sticker(self.client.state(), item, *index)
            }
        }
    }

    /// The pictures the cards show, for the pictures asked for.
    pub fn info_card_pictures(&self) -> Vec<String> {
        let mut urls = Vec::new();
        if let Some(role) = &self.client.state().role {
            urls.extend(
                role.members
                    .iter()
                    .flatten()
                    .filter_map(|member| member.avatar.clone()),
            );
        }
        if let Some(server) = self.card_server() {
            urls.extend(server.icon.clone());
            urls.extend(server.banner.clone());
        }
        if let Some((expression, _)) = &self.cards.expression
            && let Some(card) = self.expression_card(expression)
        {
            // A sticker's picture is its message's, which the timeline has.
            if matches!(expression, Expression::Emoji(_)) {
                urls.extend(card.picture);
            }
            urls.extend(card.server.and_then(|server| server.icon));
        }
        urls
    }

    /// An emoji's or sticker's card, the same way.
    pub fn expression_card_layers(&self) -> Option<[Element<'_, Message>; 2]> {
        let card = self.expression_view()?;
        Some([
            controls::outside(Message::Info(Event::CloseExpression), false),
            card,
        ])
    }

    fn expression_view(&self) -> Option<Element<'_, Message>> {
        let (expression, spot) = self.cards.expression.as_ref()?;
        let card = self.expression_card(expression)?;
        let theme = &self.theme;
        let state = self.client.state();
        let side = EXPRESSION_PICTURE;
        let size = Size::new(side, side);
        let picture: Element<'_, Message> = match expression {
            Expression::Emoji(Target::Unicode(emoji)) => {
                match emojis::get(emoji).map(emojis::Emoji::as_str) {
                    Some(key) if let Some(picture) = self.emoji_picture(key, side as u32) => {
                        crate::widgets::Pic::square(picture, side).into()
                    }
                    _ => container(text(emoji.clone()).size(EXPRESSION_GLYPH))
                        .center(side)
                        .into(),
                }
            }
            Expression::Emoji(Target::Custom(_)) => {
                let picture = card
                    .picture
                    .as_deref()
                    .and_then(|url| state.pictures.get(url));
                self.shot(Shot::new(picture, size).fit(Fit::Contain).bare())
            }
            Expression::Sticker(item, index) => {
                let picture = state
                    .message(item)
                    .and_then(|message| message.attachments.get(*index))
                    .and_then(|attachment| attachment.preview.as_ref());
                self.shot(Shot::new(picture, size).fit(Fit::Contain).bare())
            }
        };
        let words = column![
            container(elide(card.name).size(16).font(BOLD).color(theme.text))
                .height(20)
                .align_y(Alignment::Center),
            text(card.description)
                .size(14)
                .color(theme.secondary)
                .width(Length::Fill),
        ]
        .width(EXPRESSION_WORDS);
        let top = row![container(picture).padding(Padding::ZERO.top(4)), words]
            .spacing(10)
            .padding(16);
        let mut body = column![top];
        if let Some(server) = card.server {
            let sticker = matches!(expression, Expression::Sticker(..));
            body = body.push(self.expression_server(server, sticker));
        }
        let card = frame(theme, theme.raised, EXPRESSION_WIDTH, 8.0, body);
        // 4 px under it, or above it without room, as the Slint client's.
        let anchor = crate::widgets::Anchor::Span(spot.height);
        Some(
            Place::new(opaque(card), spot.position(), 8.0)
                .anchor(anchor)
                .into(),
        )
    }

    /// The server an emoji's or sticker's card names, on the panel's
    /// ground at its foot: a click opens it.
    fn expression_server(
        &self,
        server: expression::CardServer,
        sticker: bool,
    ) -> Element<'_, Message> {
        let theme = &self.theme;
        let icon_side = EXPRESSION_ICON;
        let picture = server
            .icon
            .as_deref()
            .and_then(|url| self.client.state().pictures.get(url));
        // A sticker's card shows the icon as it is, or a round letter; an
        // emoji's rounds the icon's corners.
        let (radius, ground, weight) = if sticker {
            (icon_side / 2.0, theme.overlay, SEMIBOLD)
        } else {
            (8.0, theme.field, BOLD)
        };
        let icon: Element<'_, Message> = match picture {
            Some(picture) => self.shot(
                Shot::new(Some(picture), Size::new(icon_side, icon_side))
                    .fit(if sticker { Fit::Contain } else { Fit::Cover })
                    .radius(if sticker { 0.0 } else { radius })
                    .bare(),
            ),
            None => container(
                text(crate::pictures::initials(&server.name))
                    .size(14)
                    .font(weight)
                    .color(theme.text),
            )
            .center(icon_side)
            .style(move |_| rounded(ground, radius))
            .into(),
        };
        let counts = server.counts();
        let mut lines = column![
            container(elide(server.name).size(16).font(BOLD).color(theme.text))
                .height(20)
                .align_y(Alignment::Center)
        ]
        .spacing(2);
        if !counts.is_empty() {
            lines = lines.push(
                container(elide(counts).size(12).color(theme.text))
                    .height(16)
                    .align_y(Alignment::Center),
            );
        }
        let hover = theme.hover;
        let row = button(row![icon, lines].spacing(8).align_y(Alignment::Center))
            .padding(0)
            .width(Length::Fill)
            .on_press(Message::Info(Event::OpenServer(server.id)))
            .style(crate::controls::lit_style(None, hover, 6.0));
        let heading = if sticker {
            expression::STICKER_FROM
        } else {
            expression::EMOJI_FROM
        };
        let panel = theme.panel;
        container(
            column![elide(heading).size(12).font(MEDIUM).color(theme.muted), row]
                .spacing(8)
                .padding(16),
        )
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(panel.into()),
            border: Border {
                radius: Radius {
                    bottom_left: 7.0,
                    bottom_right: 7.0,
                    ..Radius::default()
                },
                ..Border::default()
            },
            ..Default::default()
        })
        .into()
    }

    /// A role's card over a layer that closes it on a click outside.
    pub fn role_card_layers(&self) -> Option<[Element<'_, Message>; 2]> {
        let card = self.role_card()?;
        Some([
            controls::outside(Message::Info(Event::CloseRole), false),
            card,
        ])
    }

    /// A mutual server's card, the same way.
    pub fn server_card_layers(&self) -> Option<[Element<'_, Message>; 2]> {
        let card = self.server_card()?;
        Some([
            controls::outside(Message::Info(Event::CloseServer), false),
            card,
        ])
    }

    fn role_card(&self) -> Option<Element<'_, Message>> {
        let role = self.client.state().role.as_ref()?;
        let at = self.cards.role_at?;
        let theme = &self.theme;
        let ink = role.color.map(crate::style::hex);
        let dot = controls::bar(ink.unwrap_or(theme.muted), 12, 12).style({
            let ink = ink.unwrap_or(theme.muted);
            move |_| rounded(ink, 6)
        });
        let name = elide(format!("@{}", role.name))
            .size(16)
            .font(BOLD)
            .color(ink.unwrap_or(theme.text));
        let mut card = column![
            container(row![dot, name].spacing(8).align_y(Alignment::Center)).padding(Padding {
                bottom: 4.0,
                ..Padding::ZERO
            })
        ]
        .spacing(4)
        .padding(12);
        let note = match &role.members {
            None => Some("Loading..."),
            Some(members) if members.is_empty() => {
                Some("Yapfast hasn't seen anyone with this role yet.")
            }
            Some(_) => None,
        };
        if let Some(note) = note {
            card = card.push(text(note).size(13).color(theme.muted));
        }
        if let Some(members) = role.members.as_ref().filter(|members| !members.is_empty()) {
            let hover = theme.hover;
            let mut list = column![];
            for member in members {
                let face = row![
                    self.avatar(
                        member.avatar.as_deref(),
                        &member.name,
                        ROLE_AVATAR,
                        ROLE_AVATAR / 2.0,
                        1.0
                    ),
                    elide(member.name.as_str())
                        .size(15)
                        .font(MEDIUM)
                        .color(member.color.map_or(theme.text, crate::style::hex)),
                ]
                .spacing(10)
                .align_y(Alignment::Center);
                list = list.push(
                    button(face)
                        .padding([0, 6])
                        .height(ROLE_ROW)
                        .width(Length::Fill)
                        .on_press(Message::Info(Event::RoleMember(member.id.clone())))
                        .style(crate::controls::lit_style(None, hover, 6.0)),
                );
            }
            let tall = (members.len() as f32 * ROLE_ROW).min(ROLE_LIST);
            card = card.push(scrollable(list).height(tall).style(scrollbar(theme)));
        }
        let limit = self.window.height - 16.0;
        let card = frame(theme, theme.raised, ROLE_WIDTH, 8.0, card).max_height(limit);
        Some(Place::new(opaque(card), at, 8.0).into())
    }

    /// The mutual server whose card is open, as the profile lists it.
    fn card_server(&self) -> Option<&yapfast_core::chat::ProfileServer> {
        let (id, _) = self.cards.server.as_ref()?;
        let shown = self.profiles.shown.as_ref()?;
        shown
            .look
            .profile()
            .servers
            .iter()
            .find(|server| &server.id == id)
    }

    fn server_card(&self) -> Option<Element<'_, Message>> {
        let server = self.card_server()?;
        let (_, at) = self.cards.server.as_ref()?;
        let theme = &self.theme;
        let picture = |url: Option<&str>| url.and_then(|url| self.client.state().pictures.get(url));
        let banner_size = Size::new(SERVER_WIDTH - 2.0, SERVER_BANNER);
        let raised = theme.raised;
        let banner: Element<'_, Message> = match picture(server.banner.as_deref()) {
            Some(banner) => self.shot(Shot {
                picture: Some(banner),
                size: banner_size,
                fit: Fit::Cover,
                radius: 0.0,
                covered: false,
                ground: true,
            }),
            None => controls::bar(raised, banner_size.width, banner_size.height).into(),
        };
        let icon: Element<'_, Message> = match picture(server.icon.as_deref()) {
            Some(icon) => self.shot(Shot {
                picture: Some(icon),
                size: Size::new(SERVER_ICON, SERVER_ICON),
                fit: Fit::Cover,
                radius: 18.0,
                covered: false,
                ground: true,
            }),
            None => container(
                text(crate::pictures::initials(&server.name))
                    .size(24)
                    .font(BOLD)
                    .color(theme.text),
            )
            .center(SERVER_ICON)
            .style(move |_| rounded(raised, 18))
            .into(),
        };
        let ground = theme.sidebar;
        let ring = container(icon)
            .center(SERVER_ICON + 2.0 * SERVER_RING)
            .style(move |_| rounded(ground, 22));
        let top = stack![
            column![banner],
            controls::placed(ring, 10.0, SERVER_BANNER - 32.0 - SERVER_RING),
        ]
        .height(SERVER_BANNER + 32.0 + 8.0);
        let faint = theme.faint;
        let count = |words: String| {
            row![
                controls::bar(faint, 8, 8).style(move |_| rounded(faint, 4)),
                text(words).size(13).color(theme.secondary)
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        };
        let mut counts = row![].spacing(14);
        if let Some(members) = server.members {
            counts = counts.push(count(count_of(members, "member", "members")));
        }
        counts = counts.push(count(count_of(
            server.channels as u64,
            "channel",
            "channels",
        )));
        let mut body = column![
            column![
                elide(server.name.as_str())
                    .size(20)
                    .font(BOLD)
                    .color(theme.text),
                counts
            ]
            .spacing(4)
        ]
        .spacing(14)
        .padding(Padding {
            left: 16.0,
            right: 16.0,
            bottom: 16.0,
            top: 0.0,
        });
        if let Some(nick) = &server.nick {
            body = body.push(
                column![
                    text("THEIR NICKNAME")
                        .size(12)
                        .font(BOLD)
                        .color(theme.muted),
                    elide(nick.as_str()).size(14).color(theme.text),
                ]
                .spacing(4),
            );
        }
        body = body.push(
            controls::action(
                theme,
                "Go to server",
                Kind::Primary,
                Some(Message::Info(Event::GoToServer(server.id.clone()))),
            )
            .wide(36.0),
        );
        let card = frame(theme, ground, SERVER_WIDTH, 12.0, column![top, body]);
        let at = Point::new(at.x + 8.0, at.y - 8.0);
        Some(Place::new(opaque(card), at, 8.0).into())
    }
}

/// "1 member", "12 members".
fn count_of(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}
