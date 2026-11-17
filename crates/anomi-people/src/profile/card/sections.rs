//! The card's sections under its head, in Discord's order
//! (`ui/people.slint:1606`-`:1977`): the name, the full profile's actions,
//! the badges, what they're doing, what they share with this account, the
//! bio, the dates, the roles, then the full profile's connections and note,
//! and the card's Edit Profile or message box.

use iced::widget::{Column, Row, column, container, mouse_area, row, space, stack, text};
use iced::{Alignment, Background, Border, Fill, Padding, mouse};
use yapfast_lang::{tr, trn};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::people::activity::card::Intent as ActivityIntent;
use anomi_vm::people::profile::{Card, Intent, Place, Tab};
use anomi_vm::people::{self};
use anomi_vm::text::Link;
use anomi_widgets::avatar::portrait;
use anomi_widgets::paragraph::{Fold, paragraph};

use super::extra;
use super::look::{Inks, Size, Tone};
use super::name;
use super::parts::{self, Server};
use crate::activity::card as activity;
use crate::profile::badges as badge_rows;

/// The room the card keeps under a bio (`:1767`).
const BIO_FOOT: f32 = 12.0;

pub fn view<'a>(
    card: &'a Card,
    place: Place,
    size: Size,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let large = place.large();
    let mut sections = Column::<Element<'a, people::Intent>>::new()
        .spacing(size.gap)
        .padding(Padding::from([0.0, size.pad]))
        .width(Fill)
        .push(name::view(card, place, size, inks, motion));
    if large {
        if let Some(actions) = extra::actions(card, inks) {
            sections = sections.push(actions);
        }
    } else if !card.badges.badges.is_empty() {
        sections = sections.push(
            badge_rows::view(&card.badges, motion)
                .map(people::Intent::from)
                .boxed(),
        );
    }
    if !large && !card.activities.is_empty() {
        sections = sections.push(activities(card, place, motion));
    }
    if !large && !card.own && (card.mutual.friends > 0 || card.mutual.servers > 0) {
        sections = sections.push(mutual(card, place, inks, motion));
    }
    if let Some(bio) = &card.bio {
        let ink = inks.ink;
        let mut words = paragraph(bio)
            .size(if place.full() { 14.0 } else { 13.0 })
            .color(move |palette| ink.at(palette))
            .on_link(|link| match link {
                Link::Url(url) => Intent::OpenLink(url).into(),
                link => Intent::Link(link).into(),
            })
            .motion(motion);
        words = match place {
            Place::Popout => words.fold(
                Fold {
                    lines: 3,
                    beyond: 3,
                    open: false,
                },
                full_bio(place, ink),
            ),
            Place::Preview => words.max_lines(3),
            Place::Panel | Place::Side => words,
        };
        // Slint's bio column kept two 6 px gaps under the words, round its
        // hidden measure and its "View full bio", even when that is empty.
        sections = sections.push(
            container(words.boxed())
                .padding(Padding {
                    bottom: BIO_FOOT,
                    ..Padding::ZERO
                })
                .boxed(),
        );
    }
    let joined = card.joined.as_ref().map(|joined| Server {
        date: &joined.date,
        initial: &joined.initial,
        icon: &joined.icon,
    });
    if place.full() && (!card.created.is_empty() || joined.is_some()) {
        // The full profile's has the logo only beside a server's date.
        let logo = !large || joined.is_some();
        let dates = parts::member_since(&card.created, joined, inks.ink2, logo, motion);
        sections = sections.push(parts::since(dates, size.titled, inks.ink));
    }
    if large && !card.friends_since.is_empty() {
        sections = sections.push(
            column![
                parts::title(tr!("Friends Since"), inks.ink),
                tinted(&card.friends_since, 14.0, inks.ink2)
            ]
            .spacing(size.titled)
            .boxed(),
        );
    }
    if !card.roles.is_empty() {
        sections = sections.push(roles(card, place, size, inks));
    }
    for part in extra::view(card, place, size, inks, motion) {
        sections = sections.push(part);
    }
    sections.boxed()
}

/// "View full bio", under a bio the popout cut short.
fn full_bio<'a>(place: Place, ink: Tone) -> Element<'a, people::Intent> {
    mouse_area(
        text(tr!("View full bio"))
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink.at(theme.palette())),
                ..text::Style::default()
            }),
    )
    .on_press(Intent::Enlarge(place).into())
    .interaction(mouse::Interaction::Pointer)
    .boxed()
}

/// Words in one of the card's inks.
pub fn tinted<'a>(words: &'a str, size: f32, ink: Tone) -> Element<'a, people::Intent> {
    text(words)
        .size(size)
        .style(move |theme: &Theme| text::Style {
            color: Some(ink.at(theme.palette())),
            ..text::Style::default()
        })
        .boxed()
}

/// Their activities on cards, in the card's colours; the more button on
/// anyone's but a bot's, not beside a DM nor in a preview.
fn activities<'a>(card: &'a Card, place: Place, motion: Motion) -> Element<'a, people::Intent> {
    let menu = !card.bot && place == Place::Popout;
    let look = Inks::activity(card);
    let own = card.own;
    Column::with_children(card.activities.iter().map(|shown| {
        let privacy = own && shown.game;
        activity::view(shown, card.now, look, menu, motion)
            .map(move |intent: ActivityIntent| Intent::Activity { intent, privacy }.into())
    }))
    .spacing(12)
    .boxed()
}

/// "1 Mutual Friend • 2 Mutual Servers" in small text, the first three
/// friends' faces before it; each part opens the full profile on its tab.
fn mutual<'a>(
    card: &'a Card,
    place: Place,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    const FACE: f32 = 16.0;
    const STEP: f32 = 12.0;
    let ink2 = inks.ink2;
    let words = move |words: String| {
        text(words)
            .size(typography::size::CAPTION)
            .font(typography::MEDIUM)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink2.at(theme.palette())),
                ..text::Style::default()
            })
    };
    let opens = move |part: Element<'a, people::Intent>, tab| {
        mouse_area(part)
            .on_press(Intent::EnlargeOn(place, tab).into())
            .interaction(mouse::Interaction::Pointer)
            .boxed()
    };
    let mut line = Row::<Element<'a, people::Intent>>::new()
        .spacing(4)
        .height(20)
        .align_y(Alignment::Center);
    let (friends, servers) = (card.mutual.friends, card.mutual.servers);
    if friends > 0 {
        let ground = inks.ground;
        let mut faces = stack![space().width(STEP * card.mutual.faces.len().min(3) as f32 + 6.0)];
        for (index, face) in card.mutual.faces.iter().take(3).enumerate() {
            let ringed = container(portrait(face, FACE).motion(motion).view())
                .center(FACE + 2.0)
                .style(move |theme: &Theme| container::Style {
                    background: Some(Background::Color(ground.at(theme.palette()))),
                    border: Border::default().rounded(FACE / 2.0 + 1.0),
                    ..container::Style::default()
                });
            faces = faces.push(iced::widget::pin(ringed).x(index as f32 * STEP).boxed());
        }
        let said = words(trn!("{n} Mutual Friend", "{n} Mutual Friends", friends));
        line = line.push(opens(
            row![faces.height(FACE + 2.0), said]
                .spacing(4)
                .align_y(Alignment::Center)
                .boxed(),
            Tab::Friends,
        ));
    }
    if friends > 0 && servers > 0 {
        line = line.push(words("\u{2022}".to_owned()).boxed());
    }
    if servers > 0 {
        let said = words(trn!("{n} Mutual Server", "{n} Mutual Servers", servers));
        line = line.push(opens(said.boxed(), Tab::Servers));
    }
    line.boxed()
}

/// Their roles as chips, wrapping; titled where the card shows it all.
fn roles<'a>(card: &'a Card, place: Place, size: Size, inks: &Inks) -> Element<'a, people::Intent> {
    let (ink, rim) = (inks.ink, inks.ink);
    let chips = card.roles.iter().map(|role| {
        let dot = role.color;
        let mark = container(space().width(12).height(12)).style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(dot.unwrap_or(palette.faint))),
                border: Border::default().rounded(6.0),
                ..container::Style::default()
            }
        });
        let name = text(&role.name)
            .size(typography::size::CAPTION)
            .font(typography::SEMIBOLD)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink.at(theme.palette())),
                ..text::Style::default()
            });
        container(row![mark, name].spacing(6).align_y(Alignment::Center))
            .height(24)
            .padding(Padding {
                left: 6.0,
                right: 8.0,
                ..Padding::ZERO
            })
            .align_y(Alignment::Center)
            .style(move |theme: &Theme| container::Style {
                border: Border::default()
                    .rounded(8.0)
                    .width(1.0)
                    .color(rim.at(theme.palette()).scale_alpha(0.2)),
                ..container::Style::default()
            })
            .boxed()
    });
    let chips = Row::with_children(chips)
        .spacing(4)
        .wrap()
        .vertical_spacing(size.titled);
    if place.full() {
        column![parts::title(tr!("Roles"), inks.ink), chips]
            .spacing(size.titled)
            .boxed()
    } else {
        chips.boxed()
    }
}
