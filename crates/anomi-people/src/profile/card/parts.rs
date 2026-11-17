//! The small parts of a profile that other cards share: a section's title
//! (`ProfileTitle`, `ui/people.slint:603`), a list's heading in capitals
//! (`ProfileHeading`, `:607`), the "Member Since" dates (`MemberSince`,
//! `:613`), and the full profile's buttons under the name
//! (`ProfileAction`, `:1230`).

use iced::widget::{Row, button, container, hover, row, text};
use iced::{Alignment, Background, Border, Color, Padding};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use super::look::Tone;

/// A section's title: 13 px, medium, in the card's ink.
pub fn title<'a, M: 'a>(words: &'a str, ink: Tone) -> Element<'a, M> {
    text(words)
        .size(typography::size::HEADING)
        .font(typography::MEDIUM)
        .style(move |theme: &Theme| text::Style {
            color: Some(ink.at(theme.palette())),
            ..text::Style::default()
        })
        .boxed()
}

/// A list's heading: 12 px, bold, muted, spaced out.
pub fn heading<'a, M: 'a>(words: String, ink: Tone) -> Element<'a, M> {
    text(words)
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .style(move |theme: &Theme| text::Style {
            color: Some(ink.at(theme.palette())),
            ..text::Style::default()
        })
        .boxed()
}

/// The server a "Member Since" line names beside its date.
pub struct Server<'a> {
    pub date: &'a str,
    pub initial: &'a str,
    pub icon: &'a Expected,
}

/// The dates in one row, as Discord's: the Discord logo (when `logo`) and
/// the account's date, a bullet, then the server's icon, or its initial,
/// and the date of joining it.
pub fn member_since<'a, M: 'a>(
    created: &'a str,
    joined: Option<Server<'a>>,
    ink: Tone,
    logo: bool,
    motion: Motion,
) -> Element<'a, M> {
    const SIZE: f32 = 14.0;
    const MARK: f32 = 16.0;
    let words = move |words: &'a str| {
        text(words)
            .size(SIZE)
            .wrapping(text::Wrapping::None)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink.at(theme.palette())),
                ..text::Style::default()
            })
    };
    let mut line = Row::<Element<'a, M>>::new()
        .spacing(8)
        .align_y(Alignment::Center);
    if !created.is_empty() {
        if logo {
            line = line.push(
                icon(Icon::Discord, MARK)
                    .style(move |theme: &Theme, _| iced::widget::svg::Style {
                        color: Some(ink.at(theme.palette())),
                    })
                    .boxed(),
            );
        }
        line = line.push(words(created).boxed());
    }
    if let Some(server) = joined {
        if !created.is_empty() {
            line = line.push(words("\u{2022}").boxed());
        }
        let mark: Element<'a, M> = match server.icon {
            Expected::Ready(handle) => picture(handle.clone())
                .side(MARK)
                .fit(Fit::Cover)
                .radius(4.0)
                .motion(motion)
                .boxed(),
            _ => container(text(server.initial).size(8).font(typography::BOLD).style(
                move |theme: &Theme| text::Style {
                    color: Some(ink.at(theme.palette())),
                    ..text::Style::default()
                },
            ))
            .center(MARK)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().raised)),
                border: Border::default().rounded(4.0),
                ..container::Style::default()
            })
            .boxed(),
        };
        line = line.push(mark).push(words(server.date).boxed());
    }
    line.boxed()
}

/// One of the full profile's buttons: words with an icon, or a round
/// icon alone; the accent's when `primary`, else a tile of the card's.
pub struct Action<'a> {
    pub words: &'a str,
    pub glyph: Icon,
    pub primary: bool,
    pub enabled: bool,
    pub ink: Tone,
    pub tile: Tone,
    pub tile_hover: Tone,
}

impl<'a> Action<'a> {
    /// It sends `message` when clicked.
    pub fn press<M: Clone + 'a>(self, message: M) -> Element<'a, M> {
        let enabled = self.enabled;
        let face = button(hover(self.face(false), self.face(true)))
            .padding(0)
            .style(catalog::button::bare);
        if enabled {
            face.on_press(message).boxed()
        } else {
            self.face(false)
        }
    }

    /// It opens a menu under it, sending where it is.
    pub fn opens<M: 'a>(self, message: impl Fn(iced::Rectangle) -> M + 'a) -> Element<'a, M> {
        opener(hover(self.face(false), self.face(true)))
            .on_press(message)
            .boxed()
    }

    fn face<M: 'a>(&self, hovered: bool) -> Element<'a, M> {
        let round = self.words.is_empty();
        let (primary, enabled) = (self.primary, self.enabled);
        let lit = hovered && enabled;
        let (tile, tile_hover, ink) = (self.tile, self.tile_hover, self.ink);
        // A faded button fades its colours: iced has no layer opacity.
        let fade = if enabled { 1.0 } else { 0.5 };
        let face = move |palette: &anomi_theme::Palette| -> Color {
            if primary {
                palette.on_accent
            } else {
                ink.at(palette)
            }
            .scale_alpha(fade)
        };
        let mut content = row![].spacing(6).align_y(Alignment::Center).push(
            icon(self.glyph, if round { 18.0 } else { 16.0 })
                .style(move |theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(face(theme.palette())),
                })
                .boxed(),
        );
        if !round {
            content = content.push(
                text(self.words)
                    .size(typography::size::SMALL)
                    .font(typography::SEMIBOLD)
                    .style(move |theme: &Theme| text::Style {
                        color: Some(face(theme.palette())),
                        ..text::Style::default()
                    })
                    .boxed(),
            );
        }
        let pad = if round { 7.0 } else { 12.0 };
        container(content)
            .height(32)
            .padding(Padding::from([0.0, pad]))
            .align_y(Alignment::Center)
            .style(move |theme: &Theme| {
                let palette = theme.palette();
                let ground = match (primary, lit) {
                    (true, true) => palette.accent_hover,
                    (true, false) => palette.accent,
                    (false, true) => tile_hover.at(palette),
                    (false, false) => tile.at(palette),
                };
                container::Style {
                    background: Some(Background::Color(ground.scale_alpha(fade))),
                    border: Border::default().rounded(8.0),
                    ..container::Style::default()
                }
            })
            .boxed()
    }
}

/// "Member Since" over the dates.
pub fn since<'a, M: 'a>(dates: Element<'a, M>, gap: f32, ink: Tone) -> Element<'a, M> {
    iced::widget::column![title(tr!("Member Since"), ink), dates]
        .spacing(gap)
        .boxed()
}
