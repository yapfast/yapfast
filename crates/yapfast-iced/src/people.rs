//! The pieces every list of people is built from: a person's row (the
//! avatar with its presence and decoration, the name in its style, the line
//! under it), the name itself, the voice channel someone is in, and the
//! small heading over a list. The member list, the DMs, the friends and
//! requests, the profile's mutual friends, Active Now and who reacted all
//! draw their people with these.

use std::{borrow::Cow, collections::HashMap};

use iced::{
    Alignment, Color, Element, Font, Length,
    widget::{Space, button, column, container, row, text, tooltip},
};
use yapfast_core::themes::content::{NameColours, Names};
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

/// Applies an opted-in theme theme's name-colour policy to a person's own
/// colour. `Clamp` keeps the name's hue while holding its saturation and
/// lightness in the theme's ranges, then makes it readable on the chat
/// ground, as the Slint theme-content adapter did.
pub(crate) fn themed_name_colour(
    names: &Names,
    key: Option<&str>,
    own: Color,
    theme: &crate::theme::Theme,
) -> Color {
    // The theme's colours keep the name's fade (an offline member's).
    let faded = |colour: u32| {
        let colour = crate::themes::color(yapfast_core::theme::Colour::from_argb(colour));
        colour.scale_alpha(own.a)
    };
    match names.colours {
        NameColours::Keep => own,
        NameColours::Theme => names.colour.map_or(theme.text.scale_alpha(own.a), faded),
        NameColours::Clamp => clamp_name_colour(names, own, theme),
        NameColours::Palette => key
            .and_then(|key| names.palette_colour(key))
            .map_or(own, faded),
    }
}

fn clamp_name_colour(names: &Names, own: Color, theme: &crate::theme::Theme) -> Color {
    let ((low_s, high_s), (low_l, high_l)) = names.clamp_ranges(theme.dark);
    let (hue, saturation, lightness) = to_hsl(color_rgb(own));
    let clamped = pack(from_hsl(
        hue,
        saturation.clamp(f64::from(low_s), f64::from(high_s)),
        lightness.clamp(f64::from(low_l), f64::from(high_l)),
    ));
    let ground = theme.package_colour("chat").unwrap_or(theme.chat);
    let readable = unpack(readable(clamped, pack(color_rgb(ground)), 4.5));
    Color {
        r: (readable.0 / 255.0) as f32,
        g: (readable.1 / 255.0) as f32,
        b: (readable.2 / 255.0) as f32,
        a: own.a,
    }
}

type Rgb = (f64, f64, f64);

fn unpack(rgb: u32) -> Rgb {
    (
        f64::from((rgb >> 16) as u8),
        f64::from((rgb >> 8) as u8),
        f64::from(rgb as u8),
    )
}

fn pack((r, g, b): Rgb) -> u32 {
    let byte = |value: f64| u32::from(value.round().clamp(0.0, 255.0) as u8);
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

fn color_rgb(colour: Color) -> Rgb {
    (
        f64::from(colour.r) * 255.0,
        f64::from(colour.g) * 255.0,
        f64::from(colour.b) * 255.0,
    )
}

fn to_hsl((r, g, b): Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (hue * 60.0, saturation, lightness)
}

fn from_hsl(hue: f64, saturation: f64, lightness: f64) -> Rgb {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    ((r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0)
}

fn luminance((r, g, b): Rgb) -> f64 {
    let linear = |value: f64| {
        let value = value / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn readable(colour: u32, ground: u32, ratio: f64) -> u32 {
    let ground = unpack(ground);
    let lighter = luminance(ground) <= 0.5;
    let mut colour = unpack(colour);
    for _ in 0..10 {
        if contrast(colour, ground) >= ratio {
            break;
        }
        let (hue, saturation, lightness) = to_hsl(colour);
        if lighter && lightness >= 0.95 || !lighter && lightness <= 0.05 {
            break;
        }
        let step = if lighter { 0.05 } else { -0.05 };
        colour = unpack(pack(from_hsl(hue, saturation, lightness + step)));
    }
    pack(colour)
}

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
    pub font_role: &'static str,
    pub style: Option<&'a NameLook>,
    pub key: Option<&'a str>,
    /// A theme list's own name ink, which beats the theme's content
    /// colours and a name style's, as Slint's `MemberLists.name-ink`.
    pub fixed: bool,
}

impl NameInk<'_> {
    pub fn new(size: f32, weight: u16, color: Color) -> Self {
        Self {
            size,
            weight,
            color,
            font_role: "names",
            style: None,
            key: None,
            fixed: false,
        }
    }
}

impl<'a> NameInk<'a> {
    pub fn style(mut self, style: Option<&'a NameLook>) -> Self {
        self.style = style;
        self
    }

    pub fn font_role(mut self, role: &'static str) -> Self {
        self.font_role = role;
        self
    }

    pub fn key(mut self, key: &'a str) -> Self {
        self.key = Some(key);
        self
    }

    pub fn fixed(mut self, fixed: bool) -> Self {
        self.fixed = fixed;
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
    /// What someone is up to on one line: the activity's icon (the game
    /// controller in the online green) and a bullet, a custom emoji's
    /// picture, then the words, cut with "…" where the row ends.
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
            // Discord draws every activity's icon in the online green.
            let icon = self.look.icons.coloured(icon, side as u32, rgb(theme.live));
            line = line.push(Pic::square(icon, side).opacity(look.opacity));
            // The rest of their activities, "+2": green before a custom
            // status, as Discord's.
            if status.more > 0 && status.bullet {
                line = line.push(
                    text(format!("+{}", status.more))
                        .size(look.size)
                        .font(look.font)
                        .color(faded(theme.live, look.opacity)),
                );
            }
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
        let more = (look.icon && status.more > 0 && !status.bullet && status.icon > 0)
            .then_some(status.more);
        if !status.words.is_empty() {
            let words: Cow<'a, str> = match status {
                Cow::Borrowed(status) => Cow::Borrowed(status.words.as_str()),
                Cow::Owned(status) => Cow::Owned(status.words),
            };
            line = line.push(elide(words).size(look.size).font(look.font).color(ink));
        }
        if let Some(more) = more {
            line = line.push(
                text(format!("+{more}"))
                    .size(look.size)
                    .font(look.font)
                    .color(ink),
            );
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
    /// What the theme does to people's names, pictures, cards and embeds,
    /// when the person let it (Settings → Themes); none otherwise.
    pub fn theme_content(self) -> Option<&'a yapfast_core::themes::content::Content> {
        let id = &self.look.theme.id;
        self.themes
            .content(id, self.layout.theme_content.contains(id))
    }

    /// A name on one line, in its style's font and, outside a server, its
    /// style's first colour, as the lists draw a styled name.
    pub fn person_name(
        self,
        name: impl Into<Cow<'a, str>>,
        ink: NameInk<'_>,
    ) -> Element<'a, Message> {
        let name = name.into();
        let content = self.themes.content(
            &self.look.theme.id,
            self.layout.theme_content.contains(&self.look.theme.id),
        );
        let style = if content.is_some_and(|content| {
            content.names.styles == yapfast_core::themes::content::Follow::Theme
        }) {
            None
        } else {
            ink.style
        };
        let own = match style {
            Some(style) if !style.colors.is_empty() => {
                crate::style::hex(style.main).scale_alpha(ink.color.a)
            }
            _ => ink.color,
        };
        let color = if ink.fixed {
            ink.color
        } else {
            content.map_or(own, |content| {
                themed_name_colour(&content.names, ink.key, own, self.look.theme)
            })
        };
        let shown = shown_name(&name, style.and_then(NameLook::font)).into_owned();
        let stock = style_font(style.and_then(NameLook::font), ink.weight);
        let font = if style.is_some() {
            stock
        } else {
            self.look.theme.font(ink.font_role, stock)
        };
        let effect = crate::theme_effects::name_hover_effect(self, &shown);
        let mut name = crate::elide::elide(shown)
            .size(self.look.theme.type_size(ink.font_role, ink.size))
            .font(font)
            .color(color)
            .light(crate::theme_effects::text_light(
                self,
                yapfast_core::themes::package::art::TextRole::Names,
            ));
        if let Some(effect) = effect {
            name = name.hover_effect(effect);
        }
        name.into()
    }

    /// The voice channel someone is in: a speaker in the live green and the
    /// channel with its server, which brightens under the pointer; compact,
    /// the speaker alone, the place in its tooltip. A click goes there.
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
        let go = Message::Side(crate::sidebar::Event::Open(room.to_owned()));
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

    /// A list's small heading: 12 px bold capitals in the muted ink, 36 px
    /// tall.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(dark: bool, chat: u32) -> crate::theme::Theme {
        let mut theme = crate::theme::Theme::new("people-name-clamp-test", dark);
        theme.chat = crate::style::hex(chat);
        theme.text = crate::style::hex(if dark { 0xffffff } else { 0x111111 });
        theme.worked_out()
    }

    #[test]
    fn the_themes_and_palettes_name_colours_keep_an_offline_names_fade() {
        let own = crate::style::hex(0x336699).scale_alpha(0.5);
        let mut names = Names {
            colours: NameColours::Theme,
            colour: Some(0xffff_0000),
            ..Names::default()
        };
        let shown = themed_name_colour(&names, Some("someone"), own, &theme(true, 0x121214));
        assert_eq!((shown.r, shown.a), (1.0, 0.5));
        names.colour = None;
        let shown = themed_name_colour(&names, Some("someone"), own, &theme(true, 0x121214));
        assert_eq!(shown.a, 0.5);
        names.colours = NameColours::Palette;
        names.palette[0] = 0xff00_ff00;
        names.palette_len = 1;
        let shown = themed_name_colour(&names, Some("someone"), own, &theme(true, 0x121214));
        assert_eq!((shown.g, shown.a), (1.0, 0.5));
    }

    #[test]
    fn clamp_uses_the_names_saturation_and_lightness_ranges() {
        let mut names = Names {
            colours: NameColours::Clamp,
            ..Names::default()
        };
        names.saturation = Some((0.25, 0.35));
        names.lightness = Some((0.65, 0.75));
        let colour = themed_name_colour(
            &names,
            None,
            crate::style::hex(0x00ffff),
            &theme(true, 0x121214),
        );
        let (hue, saturation, lightness) = to_hsl(color_rgb(colour));
        assert!((hue - 180.0).abs() < 0.5, "{hue}");
        assert!((saturation - 0.35).abs() < 0.01, "{saturation}");
        assert!((0.649..=0.751).contains(&lightness), "{lightness}");
    }

    #[test]
    fn clamp_makes_a_dark_role_colour_readable_and_preserves_opacity() {
        let names = Names {
            colours: NameColours::Clamp,
            ..Names::default()
        };
        let own = Color {
            a: 0.4,
            ..crate::style::hex(0x101040)
        };
        let ground = crate::style::hex(0x121214);
        let colour = themed_name_colour(&names, None, own, &theme(true, 0x121214));
        let (hue, saturation, lightness) = to_hsl(color_rgb(colour));
        assert!((hue - 240.0).abs() < 0.5, "{hue}");
        assert!((0.2..=0.65).contains(&saturation), "{saturation}");
        assert!(lightness >= 0.59, "{lightness}");
        assert!(contrast(color_rgb(colour), color_rgb(ground)) >= 4.5);
        assert_eq!(colour.a, own.a);
    }
}
