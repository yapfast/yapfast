//! The member list beside a server channel's timeline, as the Slint
//! client's `MemberPanel` draws it: the Activity panel of Discord's feed
//! over it, then each group's heading and its members with their looks
//! (`looks::MemberLook`), virtualized. A click opens someone's card, a
//! right click their menu (`person.rs`).

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Size, Task, Vector,
    widget::{Space, column, container, mouse_area, row, scrollable, text, tooltip},
};
use yapfast_core::{
    application::{Action, AppState},
    chat::{ActivityKind, ChatMember, Presence},
};

use crate::{
    app::{App, Message, key},
    controls::{self, bar, lit_style},
    fonts::{MEDIUM, SEMIBOLD},
    header,
    looks::{self, Chip, MemberLine, MemberLook},
    people::StatusInk,
    people::{NameInk, Person},
    pictures::{Face, Icon},
    profile,
    style::{
        CHIP, CROWN, HEADING, MEMBER_HOVER, MEMBER_OPEN, faded, fill, hex, rounded, scrollbar,
    },
    theme::rgb,
    widgets::{Pic, PlateRow, RightPress, Rows},
};

pub const WIDTH: f32 = 265.0;
/// Where each line starts after the last, and the room above the first.
pub const PITCH: f32 = 44.0;
pub const TOP: f32 = 8.0;
/// A line's height.
const ROW: f32 = 42.0;
/// Discord's boosting pink, the same in every theme.
const BOOST: u32 = 0xff73fa;
/// The Activity panel: its header, a card and the room each card keeps
/// under it, the room above and below the panel, and how many cards show
/// until it's opened.
const ACTIVITY_HEAD: f32 = 32.0;
const CARD: f32 = 68.0;
const CARD_PITCH: f32 = 76.0;
const PANEL_TOP: f32 = 8.0;
const PANEL_BOTTOM: f32 = 4.0;
const FIRST_CARDS: usize = 3;

/// What's done in the member list's Activity panel.
#[derive(Clone, Debug)]
pub enum Event {
    /// Its header: open or close it (or show the cards again).
    Toggle,
    /// Its eye: hide the cards, or show them.
    Hide,
    /// A card was clicked, by whose it is: the cards may have moved
    /// since.
    Open(String),
}

/// A card of the Activity panel, from Discord's feed, as the Slint
/// client's `MemberActivityRow`.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityCard {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// The game, artist or show, the words about when, and its picture.
    pub game: String,
    pub since: String,
    pub thumb: Option<String>,
    /// The kind's icon, as `Icon::activity` takes it.
    pub icon: u8,
    /// Going on now, which its icon and time show in green.
    pub live: bool,
}

impl App {
    pub fn members_view(&self) -> Element<'_, Message> {
        let (first, last) = self.member_scroll.range(self.lines.len(), PITCH, TOP);
        let total = TOP + self.lines.len() as f32 * PITCH + 10.0;
        let mut rows = Rows::new(total);
        for (index, line) in self.lines.iter().enumerate().take(last).skip(first) {
            let top = TOP + index as f32 * PITCH;
            let (id, row) = match line {
                MemberLine::Heading { text, icon } => {
                    (text.as_str(), self.heading_row(text, icon.as_deref()))
                }
                MemberLine::Member(look) => (look.id.as_str(), self.member_row(look)),
            };
            rows.push(key(id), top, ROW, container(row).padding([0, 8]));
        }
        // As the Slint client's `MemberPanel` with its Discord look: the
        // chat's ground, an outline down its left edge.
        let (ground, edge) = (self.theme.chat, self.theme.outline_chat);
        let list = crate::widgets::Settle::new(
            scrollable(rows)
                .on_scroll(Message::Members)
                .height(Length::Fill)
                .style(scrollbar(&self.theme)),
            |message| match message {
                Message::Members(viewport) => Some(viewport),
                _ => None,
            },
        );
        // What the list is waiting for, or why it's empty, as the Slint
        // client's note over it.
        let note = match &self.client.state().members {
            None => Some("Loading members..."),
            Some(Err(error)) => Some(*error),
            Some(Ok(members)) if members.is_empty() => Some("No one else has joined yet."),
            Some(Ok(_)) => None,
        };
        let note = note.map(|note| {
            container(
                text(note)
                    .size(13)
                    .color(self.theme.muted)
                    .width(Length::Fill),
            )
            .padding(16)
        });
        let list: Element<'_, Message> = column![]
            .push(self.activity_panel())
            .push(note)
            .push(list)
            .into();
        container(row![bar(edge, 1, Length::Fill), list])
            .width(WIDTH)
            .height(Length::Fill)
            .style(move |_| fill(ground))
            .into()
    }

    /// A group's heading: 14 px medium in the channel grey, 20 px above,
    /// its role's icon before it.
    fn heading_row(&self, heading: &str, icon: Option<&str>) -> Element<'_, Message> {
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(icon) = icon.and_then(|icon| self.picture(icon, 20, 20, false)) {
            line = line.push(Pic::square(icon, 20.0));
        }
        line = line.push(
            text(heading.to_owned())
                .size(14)
                .font(MEDIUM)
                .color(hex(HEADING)),
        );
        container(line)
            .padding(Padding {
                top: 20.0,
                right: 0.0,
                bottom: 4.0,
                left: 8.0,
            })
            .height(ROW)
            .align_y(Alignment::Center)
            .into()
    }

    /// A member's row: the face with its decoration (playing while pointed
    /// at) and presence, the name in its style's font and its role's
    /// colour, the role icon, server tag, APP tag, crown, boosting gem and
    /// timeout's clock, the activity line, and the nameplate behind it all.
    fn member_row<'a>(&'a self, look: &'a MemberLook) -> Element<'a, Message> {
        let theme = &self.theme;
        let pointed = self.pointed.member.as_ref() == Some(&look.id);
        // The member whose card is open.
        let selected = self.card.as_ref().is_some_and(|card| card.user == look.id);
        let offline = look.offline();
        let (avatar_opacity, opacity) = if offline { (0.4, 0.5) } else { (1.0, 1.0) };
        let face = Face::new(look.avatar.as_deref(), &look.name, 32.0)
            .opacity(avatar_opacity)
            .decoration(look.decoration.as_deref(), pointed)
            .dot(
                look.presence
                    .filter(|presence| *presence != Presence::Offline)
                    .map(|presence| (presence, rgb(theme.sidebar), Point::new(20.0, 20.0))),
            );
        let color = faded(look.color.map_or(theme.secondary, hex), opacity);
        let name = self.person_name(
            &look.name,
            NameInk::new(16.0, 500, color).style(look.style.as_ref()),
        );
        let mut name_line = row![name].spacing(4).align_y(Alignment::Center);
        if let Some(icon) = look
            .role_icon
            .as_deref()
            .and_then(|icon| self.picture(icon, 16, 16, false))
        {
            name_line = name_line.push(Pic::square(icon, 16.0).opacity(opacity));
        }
        if let Some((letters, badge)) = &look.tag {
            name_line = name_line.push(self.tag_chip(
                letters,
                badge.as_deref(),
                Chip {
                    size: 12.0,
                    font: SEMIBOLD,
                    height: 16.0,
                    pad: 4.0,
                    radius: 4.0,
                    fill: CHIP,
                    ink: theme.text,
                },
                opacity,
            ));
        }
        if look.bot {
            name_line = name_line.push(self.bot_tag(opacity));
        }
        if look.owner {
            let crown = self.icon(Icon::Crown, 14, CROWN);
            name_line = name_line.push(Pic::square(crown, 14.0).opacity(opacity));
        }
        if let Some(boosting) = &look.boosting
            && self.layout.prefs.boost_gems.unwrap_or(true)
        {
            let gem = Pic::square(self.icon(Icon::Gem, 14, BOOST), 14.0);
            name_line = name_line.push(
                tooltip(
                    gem,
                    controls::tip(theme, boosting.as_str()),
                    tooltip::Position::Top,
                )
                .gap(6),
            );
        }
        if look.timed_out {
            name_line = name_line.push(self.glyph(Icon::Clock, 14, theme.danger));
        }
        let mut person = Person::new(face, name_line);
        if !look.status.is_empty() {
            person = person
                .sub(self.status_line(&look.status, StatusInk::under_name(&self.theme, opacity)));
        }
        let plate = look
            .nameplate
            .as_ref()
            .and_then(|(url, _)| self.nameplate(url, ROW, pointed, selected, opacity));
        let content = self
            .person(person)
            .width(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: if look.nameplate.is_some() { 12.0 } else { 8.0 },
                bottom: 0.0,
                left: 8.0,
            })
            .height(ROW);
        // The ground the nameplate fades into, as the Slint client's
        // `PlateRow` has it.
        let ground = if selected {
            theme.active
        } else if pointed {
            theme.hover
        } else {
            theme.sidebar
        };
        let palette = look
            .nameplate
            .as_ref()
            .and_then(|(_, palette)| *palette)
            .map(hex);
        let plated = PlateRow::new(
            content,
            plate,
            palette,
            ground,
            plate_end(pointed, selected),
            opacity,
        );
        let id = look.id.clone();
        let menu_id = look.id.clone();
        let open_id = look.id.clone();
        let origin = self.member_list_origin();
        let area = mouse_area(
            container(plated)
                .width(Length::Fill)
                .height(ROW)
                .clip(true)
                .style(move |_| container::Style {
                    background: if selected {
                        Some(MEMBER_OPEN.into())
                    } else {
                        pointed.then(|| MEMBER_HOVER.into())
                    },
                    border: Border::default().rounded(8),
                    ..Default::default()
                }),
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .on_enter(Message::PointMember(id.clone(), true))
        .on_exit(Message::PointMember(id, false))
        .on_press(Message::OpenMember(open_id));
        RightPress::new(area, move |at| {
            Message::PersonMenu(menu_id.clone(), at + origin)
        })
        .into()
    }

    /// A nameplate's picture behind a row `height` tall, playing while the
    /// row is pointed at, stronger pointed at or selected.
    pub fn nameplate(
        &self,
        url: &str,
        height: f32,
        pointed: bool,
        selected: bool,
        opacity: f32,
    ) -> Option<Element<'_, Message>> {
        let strength = if pointed || selected { 0.9 } else { 0.6 } * opacity;
        self.plate_picture(url, height, strength, pointed)
    }

    /// The account's own nameplate behind its card or a tile `height`
    /// tall, with its palette's colour for the theme, as the Slint client's
    /// `Nameplate` with `account`: .4 strong, .6 when `hot`, playing when
    /// `play`. None without one, or with Show nameplates off.
    pub fn account_plate(
        &self,
        height: f32,
        hot: bool,
        play: bool,
    ) -> Option<(Element<'_, Message>, Option<Color>)> {
        let state = self.client.state();
        let plate = state
            .account_nameplate
            .as_ref()
            .filter(|_| !state.hide_nameplates)?;
        let picture =
            self.plate_picture(&plate.picture, height, if hot { 0.6 } else { 0.4 }, play)?;
        let palette = plate
            .colors
            .map(|[dark, light]| hex(if self.theme.dark { dark } else { light }));
        Some((picture, palette))
    }

    /// A nameplate's picture at a row's right end, `height` tall, at
    /// `strength`, its moving picture asked for and played when `play`.
    fn plate_picture(
        &self,
        url: &str,
        height: f32,
        strength: f32,
        play: bool,
    ) -> Option<Element<'_, Message>> {
        let picture = self.client.state().pictures.get(url)?;
        let width = (height * picture.width as f32 / picture.height.max(1) as f32).round();
        let size = Size::new(width, height);
        let moving = play
            .then(|| {
                self.moving(
                    &looks::animated_url(url),
                    size,
                    Rectangle::with_size(size),
                    false,
                )
            })
            .flatten();
        Some(match moving {
            Some(moving) => moving.radius(8.0).opacity(strength).into(),
            None => Element::from(
                Pic::new(
                    self.fit(picture, size.width as u32, size.height as u32, false)?,
                    size,
                )
                .radius(8.0)
                .opacity(strength),
            ),
        })
    }

    /// Where the member list's rows start in the window, which a right
    /// click's point is taken from.
    fn member_list_origin(&self) -> Vector {
        Vector::new(
            self.window.width - WIDTH + 1.0,
            header::HEIGHT + 1.0 + self.activity_height(),
        )
    }

    /// The Activity panel over the list: "Activity — N" with its eye and
    /// chevron, then the first three cards (all of them once it's opened),
    /// as Discord's: 8 px corners and padding, an outline, the name, the
    /// title in bold, the kind's icon with the time, the picture.
    fn activity_panel(&self) -> Option<Element<'_, Message>> {
        if self.activity_cards.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let hidden = self.client.state().activity_cards_hidden;
        let opens = self.activity_cards.len() > FIRST_CARDS || hidden;
        let mut head = row![
            text(format!("Activity \u{2014} {}", self.activity_cards.len()))
                .size(14)
                .font(MEDIUM)
                .color(theme.muted)
                .width(Length::Fill),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let (eye, eye_label) = if hidden {
            (Icon::EyeOff, "Show Activity Cards")
        } else {
            (Icon::Eye, "Hide Activity Cards")
        };
        head = head.push(self.icon_button(
            eye,
            eye_label,
            20.0,
            16,
            Message::MemberActivity(Event::Hide),
            false,
        ));
        if opens {
            let icon = if self.activity_open && !hidden {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            head = head.push(self.glyph(icon, 16, theme.muted));
        }
        let lit = theme.wash(0.06);
        let head = iced::widget::button(
            container(head)
                .padding([0, 8])
                .height(ACTIVITY_HEAD)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press_maybe(opens.then_some(Message::MemberActivity(Event::Toggle)))
        .style(lit_style(None, lit, 4.0));
        let mut panel = column![head].padding(Padding {
            top: PANEL_TOP,
            right: 8.0,
            bottom: PANEL_BOTTOM,
            left: 8.0,
        });
        for card in self.activity_cards.iter().take(self.cards_shown()) {
            panel = panel.push(container(self.activity_card(card)).padding(Padding {
                bottom: CARD_PITCH - CARD,
                ..Padding::ZERO
            }));
        }
        Some(panel.into())
    }

    /// How many of the Activity panel's cards show.
    fn cards_shown(&self) -> usize {
        if self.client.state().activity_cards_hidden {
            0
        } else if self.activity_open {
            self.activity_cards.len()
        } else {
            self.activity_cards.len().min(FIRST_CARDS)
        }
    }

    /// The Activity panel's height, which the list starts below.
    pub fn activity_height(&self) -> f32 {
        if self.activity_cards.is_empty() || !self.members_shown() {
            return 0.0;
        }
        PANEL_TOP + ACTIVITY_HEAD + self.cards_shown() as f32 * CARD_PITCH + PANEL_BOTTOM
    }

    fn activity_card<'a>(&'a self, card: &'a ActivityCard) -> Element<'a, Message> {
        let theme = &self.theme;
        let tone = if card.live { theme.live } else { theme.muted };
        let mut when = row![].spacing(4).align_y(Alignment::Center);
        if let Some(icon) = Icon::activity(card.icon) {
            when = when.push(self.glyph(icon, 16, tone));
        }
        when = when.push(
            text(card.since.as_str())
                .size(12)
                .color(tone)
                .wrapping(text::Wrapping::None),
        );
        let who = row![
            self.avatar(card.avatar.as_deref(), &card.name, 16.0, 8.0, 1.0),
            crate::elide::elide(card.name.as_str())
                .size(14)
                .font(MEDIUM)
                .color(theme.secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let words = column![
            who,
            crate::elide::elide(card.game.as_str())
                .size(14)
                .font(SEMIBOLD)
                .color(theme.text),
            when,
        ]
        .spacing(2)
        .width(Length::Fill);
        let mut body = row![words].spacing(8).align_y(Alignment::Center);
        if let Some(thumb) = &card.thumb {
            let raised = theme.raised;
            body = body.push(match self.picture(thumb, 48, 48, true) {
                Some(picture) => Element::from(Pic::square(picture, 48.0).radius(8.0)),
                None => container(Space::new())
                    .width(48)
                    .height(48)
                    .style(move |_| rounded(raised, 8))
                    .into(),
            });
        }
        let (rest, lit, line) = (theme.wash(0.06), theme.wash(0.12), theme.line);
        let face = iced::widget::button(container(body).padding(8).height(CARD).clip(true))
            .padding(0)
            .width(Length::Fill)
            .on_press(Message::MemberActivity(Event::Open(card.id.clone())))
            .style(move |_, status| iced::widget::button::Style {
                background: Some(if controls::lights(status) { lit } else { rest }.into()),
                border: crate::style::edge(line, 8.0),
                ..Default::default()
            });
        // The panel is in no scrollable: the point is the window's already.
        let user = card.id.clone();
        RightPress::new(face, move |at| Message::PersonMenu(user.clone(), at)).into()
    }

    /// What's done in the Activity panel.
    pub fn member_activity(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle => {
                if self.client.state().activity_cards_hidden {
                    return self.hide_activity(false);
                }
                self.activity_open = !self.activity_open;
                self.ask()
            }
            Event::Hide => {
                let hidden = !self.client.state().activity_cards_hidden;
                self.hide_activity(hidden)
            }
            Event::Open(id) => {
                let Some((index, card)) = self
                    .activity_cards
                    .iter()
                    .enumerate()
                    .find(|(_, card)| card.id == id)
                else {
                    return Task::none();
                };
                let top =
                    header::HEIGHT + 1.0 + PANEL_TOP + ACTIVITY_HEAD + index as f32 * CARD_PITCH;
                let at = Point::new(self.window.width - WIDTH - 8.0 - profile::WIDTH, top - 16.0);
                self.open_card(card.id.clone(), at)
            }
        }
    }

    /// Hides the Activity panel's cards or shows them, and keeps it.
    fn hide_activity(&mut self, hidden: bool) -> Task<Message> {
        crate::layout::store(self.client.root(), "activity_cards_hidden", hidden);
        self.client.dispatch(Action::ActivityCardsHidden(hidden));
        Task::batch([self.sync(), self.ask()])
    }

    /// Makes the Activity panel's cards again from the core's feed, keeping
    /// them when they come out the same.
    pub fn make_activity_cards(&mut self) {
        let state = self.client.state();
        let cards = match (&state.selected, &self.members) {
            (Some(room), Some(members)) if self.server => {
                activity_cards(state, room, members, yapfast_core::clock::now_ms())
            }
            _ => Vec::new(),
        };
        if cards != self.activity_cards {
            self.activity_cards = cards;
        }
    }

    /// A member's line was clicked: their card opens beside the list, its
    /// top 16 px above the row's.
    pub fn open_member(&mut self, id: &str) -> Task<Message> {
        // Found by id: the lines may have moved since the click.
        let Some((index, look)) =
            self.lines
                .iter()
                .enumerate()
                .find_map(|(index, line)| match line {
                    MemberLine::Member(look) if look.id == id => Some((index, look)),
                    _ => None,
                })
        else {
            return Task::none();
        };
        let top = header::HEIGHT + 1.0 + self.activity_height() + TOP + index as f32 * PITCH
            - self.member_scroll.offset;
        let at = Point::new(self.window.width - WIDTH - 8.0 - profile::WIDTH, top - 16.0);
        let user = look.id.clone();
        self.open_card(user, at)
    }

    /// The pictures the Activity panel's cards show.
    pub fn activity_pictures(&self) -> Vec<&str> {
        self.activity_cards
            .iter()
            .take(self.cards_shown())
            .flat_map(|card| [card.avatar.as_deref(), card.thumb.as_deref()])
            .flatten()
            .collect()
    }
}

/// How far a nameplate's palette reaches over its row: further pointed at,
/// furthest selected, as the Slint client's `Nameplate`.
pub fn plate_end(pointed: bool, selected: bool) -> f32 {
    if selected {
        0.5
    } else if pointed {
        0.3
    } else {
        0.2
    }
}

/// What the channel's Activity panel shows: Discord's feed, in its order,
/// of those `state.activity_people` says it may show, as the Slint client
/// words it (`activity_panel` in `src/ui/profile.rs`). Between two channels
/// of a server the last answer shows until this one's comes.
pub fn activity_cards(
    state: &AppState,
    room: &str,
    members: &[ChatMember],
    now_ms: u64,
) -> Vec<ActivityCard> {
    use yapfast_core::{
        chat::FeedEntry, discord::parse_room, yapfast_discord::activity_feed::Kind,
    };
    let Some((asked, people)) = &state.activity_people else {
        return Vec::new();
    };
    let server = |room| parse_room(room).map(|(server, _)| server);
    if server(asked) != server(room) {
        return Vec::new();
    }
    // What their presence says of it, which is more than the feed does.
    let doing = |member: &ChatMember, entry: &FeedEntry| {
        member
            .activities
            .iter()
            .find(|activity| match entry.kind {
                Kind::Game | Kind::Launched => {
                    activity.kind == ActivityKind::Playing && activity.name == entry.title
                }
                Kind::Listened => activity.kind == ActivityKind::Listening,
                Kind::Watched => activity.kind == ActivityKind::Watching,
                Kind::TopGame | Kind::TopArtist => false,
            })
            .cloned()
    };
    state
        .activity_feed
        .iter()
        .filter_map(|entry| {
            let person = people.iter().find(|person| person.id == entry.author)?;
            let live = entry.live_at(now_ms);
            let member = live
                .then(|| members.iter().find(|member| *member.id == *entry.author))
                .flatten();
            // Discord's client leaves out what someone offline "is" doing.
            if member.is_some_and(|member| member.presence == Some(Presence::Offline)) {
                return None;
            }
            let activity = member.and_then(|member| doing(member, entry));
            let since = match (entry.hours, entry.tracks) {
                (Some(hours), _) if entry.kind == Kind::TopGame => {
                    format!("Most played: {hours}h")
                }
                (_, Some(tracks)) if entry.kind == Kind::TopArtist => {
                    format!("{tracks} tracks this week")
                }
                _ if live => running_for(
                    activity
                        .as_ref()
                        .and_then(|activity| activity.start)
                        .unwrap_or(entry.at_ms),
                    now_ms,
                ),
                _ => finished_ago(entry.at_ms, now_ms),
            };
            let picture = activity
                .as_ref()
                .and_then(|activity| activity.large_image.as_deref())
                .or(entry.picture.as_deref())
                .unwrap_or_default();
            let thumb = yapfast_core::yapfast_discord::activity::small_picture(picture);
            Some(ActivityCard {
                id: person.id.to_string(),
                name: person.name.to_string(),
                avatar: (!person.avatar.is_empty()).then(|| person.avatar.to_string()),
                icon: match entry.kind {
                    Kind::Game | Kind::Launched => 1,
                    Kind::Listened => 2,
                    Kind::Watched => 3,
                    Kind::TopGame | Kind::TopArtist => 5,
                },
                live,
                game: entry.title.to_string(),
                since,
                thumb: (!thumb.is_empty()).then(|| thumb.to_string()),
            })
        })
        .collect()
}

/// A running time as the activity panel words it: "for 12 min".
fn running_for(started_ms: u64, now_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(started_ms) / 60_000;
    match minutes {
        0 => "just started".to_owned(),
        1..=59 => format!("for {minutes} min"),
        60..=1439 => format!("for {} hr", minutes / 60),
        _ => format!("for {} d", minutes / 1440),
    }
}

/// How long ago something finished as the activity panel words it, in
/// Discord's steps: "3h ago", "2d ago".
pub fn finished_ago(at_ms: u64, now_ms: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        0..MINUTE => format!("{seconds}s ago"),
        MINUTE..HOUR => format!("{}m ago", seconds / MINUTE),
        _ if seconds < 12 * HOUR => format!("{}h ago", seconds / HOUR),
        _ if seconds < 9 * DAY => format!("{}d ago", (seconds / DAY).max(1)),
        _ if seconds < 28 * DAY => format!("{}w ago", seconds / (7 * DAY)),
        _ => format!("{}mo ago", (seconds / (30 * DAY)).max(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::{finished_ago, running_for};

    #[test]
    fn the_activity_panels_times_read_in_discords_steps() {
        let now = 100 * 24 * 3_600_000;
        let ago = |seconds: u64| finished_ago(now - seconds * 1000, now);
        assert_eq!(ago(42), "42s ago");
        assert_eq!(ago(5 * 60), "5m ago");
        assert_eq!(ago(3 * 3600), "3h ago");
        // From twelve hours a day, at least one.
        assert_eq!(ago(13 * 3600), "1d ago");
        assert_eq!(ago(10 * 86_400), "1w ago");
        assert_eq!(ago(40 * 86_400), "1mo ago");
        let since = |minutes: u64| running_for(now - minutes * 60_000, now);
        assert_eq!(since(0), "just started");
        assert_eq!(since(25), "for 25 min");
        assert_eq!(since(130), "for 2 hr");
        assert_eq!(since(3 * 1440), "for 3 d");
    }
}
