//! The pieces every list of people is built from: a person's row (the
//! avatar with its presence and decoration, the name in its style, the line
//! under it), the name itself, the voice channel someone is in, and the
//! small heading over a list. The member list, the DMs, the friends and
//! requests, the profile's mutual friends, Active Now and who reacted all
//! draw their people with these, as the Slint client's `Avatar`,
//! `StyledName`, `StatusText`, `VoiceSpot` and `RequestsHeading`.

use std::{borrow::Cow, collections::HashMap};

use iced::{
    Alignment, Color, Element, Font, Length,
    widget::{Space, button, column, container, row, text, tooltip},
};
use yapfast_core::{application::AppState, chat::ChatRoom};

use crate::{
    app::Message,
    controls::{self, ink_style},
    elide::elide,
    fonts::{BOLD, REGULAR, shown_name, style_font},
    looks::{NameLook, Status},
    pictures::{Face, Icon},
    style::faded,
    theme::rgb,
    widgets::Pic,
};

/// How a status line is written: its size, weight, ink and opacity, and
/// whether the activity's icon leads it.
#[derive(Clone, Copy)]
pub struct StatusInk {
    pub size: f32,
    pub font: Font,
    pub ink: Color,
    pub opacity: f32,
    pub icon: bool,
}

impl StatusInk {
    /// Under a name in a list: 12 px in the muted ink, faded by `opacity`,
    /// the activity's icon first.
    pub fn under_name(theme: &crate::theme::Theme, opacity: f32) -> Self {
        Self {
            size: 12.0,
            font: REGULAR,
            ink: theme.muted,
            opacity,
            icon: true,
        }
    }

    /// An emoji and words alone, at a size, weight and ink of their own.
    pub fn words(size: f32, font: Font, ink: Color) -> Self {
        Self {
            size,
            font,
            ink,
            opacity: 1.0,
            icon: false,
        }
    }
}

/// The gap between a person's avatar and their name, as Discord's lists
/// leave it.
pub const GAP: f32 = 12.0;

/// A person's row: their face, then their name over the line under it,
/// centred on the face. The caller puts it in its own ground and adds what
/// follows (buttons, counts).
pub struct Person<'a> {
    pub face: Face<'a>,
    pub name: Element<'a, Message>,
    pub sub: Option<Element<'a, Message>>,
    pub gap: f32,
}

impl<'a> Person<'a> {
    pub fn new(face: Face<'a>, name: impl Into<Element<'a, Message>>) -> Self {
        Self {
            face,
            name: name.into(),
            sub: None,
            gap: GAP,
        }
    }

    /// The line under the name.
    pub fn sub(mut self, sub: impl Into<Element<'a, Message>>) -> Self {
        self.sub = Some(sub.into());
        self
    }
}

/// How a name is written: its size, weight and colour, and the style that
/// gives it its font (a server's rows) or its colour too (elsewhere).
#[derive(Clone, Copy)]
pub struct NameInk<'a> {
    pub size: f32,
    pub weight: u16,
    pub color: Color,
    pub style: Option<&'a NameLook>,
}

impl NameInk<'_> {
    pub fn new(size: f32, weight: u16, color: Color) -> Self {
        Self {
            size,
            weight,
            color,
            style: None,
        }
    }
}

impl<'a> NameInk<'a> {
    pub fn style(mut self, style: Option<&'a NameLook>) -> Self {
        self.style = style;
        self
    }
}

/// Where each person in a voice channel of the account's servers is, by
/// their id: the channel's room id, its name, and its server's name, as the
/// DMs and friends mark them.
pub fn voice_spots(state: &AppState) -> HashMap<&str, (&str, &str, &str)> {
    let rooms: HashMap<&str, &ChatRoom> = state
        .rooms
        .iter()
        .filter(|room| room.voice && !room.people.is_empty())
        .map(|room| (room.id.as_str(), &**room))
        .collect();
    let mut spots = HashMap::new();
    if rooms.is_empty() {
        return spots;
    }
    for server in &state.servers {
        for channel in &server.channels {
            let Some(room) = rooms.get(channel.as_str()) else {
                continue;
            };
            for person in &room.people {
                spots.insert(
                    person.id.as_str(),
                    (room.id.as_str(), room.name.as_str(), server.name.as_str()),
                );
            }
        }
    }
    spots
}

impl<'a> crate::ctx::Ctx<'a> {
    /// What someone is up to on one line, as the Slint client's
    /// `StatusText`: the activity's icon (the game controller in the online
    /// green) and a bullet, a custom emoji's picture, then the words, cut
    /// with "…" where the row ends.
    pub fn status_line(
        self,
        status: impl Into<Cow<'a, Status>>,
        look: StatusInk,
    ) -> Element<'a, Message> {
        let status = status.into();
        let theme = &self.look.theme;
        let mut line = row![].spacing(4).align_y(Alignment::Center);
        let side = (look.size + 3.0).round();
        let ink = faded(look.ink, look.opacity);
        if look.icon
            && let Some(icon) = Icon::activity(status.icon)
        {
            let color = rgb(if icon == Icon::Gamepad {
                theme.live
            } else {
                theme.muted
            });
            let icon = self.look.icons.coloured(icon, side as u32, color);
            line = line.push(Pic::square(icon, side).opacity(look.opacity));
            if status.bullet {
                line = line.push(text("\u{2022}").size(look.size).color(ink));
            }
        }
        if let Some(emoji) = &status.emoji {
            line = line.push(match self.picture(emoji, side as u32, side as u32, false) {
                Some(picture) => Element::from(Pic::square(picture, side).opacity(look.opacity)),
                None => Space::new().width(side).height(side).into(),
            });
        }
        if !status.words.is_empty() {
            let words: Cow<'a, str> = match status {
                Cow::Borrowed(status) => Cow::Borrowed(status.words.as_str()),
                Cow::Owned(status) => Cow::Owned(status.words),
            };
            line = line.push(elide(words).size(look.size).font(look.font).color(ink));
        }
        line.into()
    }

    /// A person's row: the face, then the name over its line.
    pub fn person(self, person: Person<'a>) -> iced::widget::Row<'a, Message> {
        let mut lines = column![person.name].width(Length::Fill);
        if let Some(sub) = person.sub {
            lines = lines.push(sub);
        }
        row![self.face(person.face), lines]
            .spacing(person.gap)
            .align_y(Alignment::Center)
    }
}

impl<'a> crate::ctx::Ctx<'a> {
    /// A name on one line, in its style's font and, outside a server, its
    /// style's first colour, as the lists draw a styled name.
    pub fn person_name(self, name: &'a str, ink: NameInk<'a>) -> Element<'a, Message> {
        let color = match ink.style {
            Some(style) if !style.colors.is_empty() => {
                crate::style::hex(style.main).scale_alpha(ink.color.a)
            }
            _ => ink.color,
        };
        crate::elide::elide(shown_name(name, ink.style.and_then(NameLook::font)))
            .size(ink.size)
            .font(style_font(ink.style.and_then(NameLook::font), ink.weight))
            .color(color)
            .into()
    }

    /// The voice channel someone is in, as the Slint client's `VoiceSpot`:
    /// a speaker in the live green and the channel with its server, which
    /// brightens under the pointer; compact, the speaker alone, the place
    /// in its tooltip. A click goes there.
    pub fn voice_spot(
        self,
        (room, channel, server): (&str, &str, &str),
        size: f32,
        compact: bool,
    ) -> Element<'a, Message> {
        let theme = self.look.theme;
        let place = format!("{channel} — {server}");
        let speaker = self
            .look
            .glyph(Icon::Volume, (size + 1.0) as u32, theme.live);
        let go = Message::Side(crate::sidebar::Event::Select(room.to_owned()));
        if compact {
            let spot = button(speaker)
                .padding(0)
                .on_press(go)
                .style(ink_style(theme.muted, theme.text));
            return tooltip(spot, controls::tip(theme, place), tooltip::Position::Top)
                .gap(6)
                .into();
        }
        button(
            row![speaker, crate::elide::elide(place).size(size)]
                .spacing(4)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press(go)
        .style(ink_style(theme.muted, theme.text))
        .into()
    }

    /// A list's small heading, as the Slint client's `RequestsHeading`:
    /// 12 px bold capitals in the muted ink, 36 px tall.
    pub fn list_heading(self, heading: String) -> Element<'a, Message> {
        container(
            text(heading)
                .size(12)
                .font(BOLD)
                .color(self.look.theme.muted),
        )
        .padding([0, 8])
        .height(36)
        .align_y(Alignment::Center)
        .into()
    }
}
