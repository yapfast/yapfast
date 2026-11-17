//! Who the card is of: the name, in its style when it has one, then the
//! username with the pronouns, the server tag, and on the full profile the
//! badges on the same line, as many as fit, six at most, then "+n", which
//! shows them all under it (`ui/people.slint:1608`-`:1681`). The name and
//! the username select, to copy, as in Discord; a styled name opens the
//! full profile, as the picture does.

use iced::widget::{Column, Row, button, container, mouse_area, responsive, rich_text, span, text};
use iced::{Alignment, Fill, Length, Padding, mouse};

use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_theme::typography;
use anomi_vm::people::badges::Badges;
use anomi_vm::people::profile::{Card, Intent, Place};
use anomi_vm::people::{self};
use anomi_widgets::name::styled_name;
use anomi_widgets::tag::server_tag;

use super::look::{Inks, Size, Tone};
use crate::profile::badges::{self as badge_rows, one, pill};

/// The username's size, and the line's spacing.
const HANDLE: f32 = 14.0;
const LINE_GAP: f32 = 6.0;
/// The tag's chip on a card: 24 px tall, 8 px in, 8 px corners, a grey
/// wash (white on a light theme) inside a faint outline (`:1640`).
const CHIP: f32 = 24.0;
const CHIP_FILL: u32 = 0x97979f1f;
const CHIP_FILL_LIGHT: u32 = 0xffffff3d;
const CHIP_OUTLINE: u32 = 0x97979f0a;
/// The inline badges: 22 px a badge, 6 px of pill, six at most, and "+n"
/// in 12 px semibold with 8 px round it (`:1641`-`:1675`).
const BADGE: f32 = 22.0;
const PILL: f32 = 6.0;
const INLINE: usize = 6;
const PLUS_SIZE: f32 = 12.0;

pub fn view<'a>(
    card: &'a Card,
    place: Place,
    size: Size,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let ink = inks.ink;
    let opens = matches!(place, Place::Popout | Place::Panel);
    let name: Element<'a, people::Intent> = match card.name_style {
        Some(style) => {
            let styled = styled_name(&card.name, Some(style))
                .size(size.name)
                .weight(iced::font::Weight::Bold)
                .ink(ink_of(ink));
            if opens {
                mouse_area(styled)
                    .on_press(Intent::Enlarge(place).into())
                    .interaction(mouse::Interaction::Pointer)
                    .boxed()
            } else {
                styled.boxed()
            }
        }
        None => selectable(card.name.as_str(), size.name, typography::BOLD, ink),
    };
    let inline = place.large() && !card.badges.badges.is_empty() && !card.touch.all_badges;
    let mut line = Row::<Element<'a, people::Intent>>::new()
        .spacing(LINE_GAP)
        .align_y(Alignment::Center);
    if !card.handle.is_empty() {
        let words: std::borrow::Cow<'a, str> = if card.pronouns.is_empty() {
            card.handle.as_str().into()
        } else {
            format!("{} \u{2022} {}", card.handle, card.pronouns).into()
        };
        line = line.push(selectable(words, HANDLE, typography::UI, ink));
    }
    if let Some(tag) = &card.tag {
        let mut chip = server_tag(tag, motion)
            .height(CHIP)
            .pad(8.0)
            .radius(8.0)
            .font(typography::UI)
            .fill(rgba(if inks.light() {
                CHIP_FILL_LIGHT
            } else {
                CHIP_FILL
            }))
            .outline(rgba(CHIP_OUTLINE), 1.0)
            .keyboard(true);
        if let Some(ink) = ink.fixed() {
            chip = chip.color(ink);
        }
        line = line.push(if place == Place::Preview {
            chip.view().boxed()
        } else {
            let server = tag.server.clone();
            chip.on_open(move |at| Intent::Tag(server.clone(), at).into())
        });
    }
    if inline {
        line = line.push(inline_badges(&card.badges, place, ink, motion));
    }
    let mut block = Column::<Element<'a, people::Intent>>::new()
        .spacing(if place.large() { 4.0 } else { 2.0 })
        .push(name);
    if !card.handle.is_empty() || card.tag.is_some() || inline {
        block = block.push(line.boxed());
    }
    if place.large() && card.touch.all_badges && !card.badges.badges.is_empty() {
        block = block.push(
            container(badge_rows::view(&card.badges, motion).map(people::Intent::from))
                .padding(Padding {
                    top: 4.0,
                    ..Padding::ZERO
                })
                .boxed(),
        );
    }
    block.boxed()
}

/// A name's ink as `styled_name` takes it.
fn ink_of(tone: Tone) -> anomi_widgets::name::Ink {
    match tone {
        Tone::Token(token) => anomi_widgets::name::Ink::Token(token),
        Tone::Fixed(color) => anomi_widgets::name::Ink::Fixed(color),
    }
}

/// Words that select, to copy.
fn selectable<'a>(
    words: impl iced::advanced::text::IntoFragment<'a>,
    size: f32,
    font: iced::Font,
    ink: Tone,
) -> Element<'a, people::Intent> {
    rich_text([span(words)])
        .size(size)
        .font(font)
        .selectable(true)
        .on_link_click(iced::never)
        .style(move |theme: &Theme| text::Style {
            color: Some(ink.at(theme.palette())),
            ..text::Style::default()
        })
        .boxed()
}

/// How many badges show on the line `width` wide: all when they fit, six
/// at most, else as many as leave room for "+n".
pub fn shown(count: usize, width: f32) -> usize {
    let fit = |room: f32| ((room - PILL) / BADGE).floor().max(0.0) as usize;
    if count <= INLINE.min(fit(width)) {
        return count;
    }
    // Room for "+n", measured on the most it could say: 8 px round a
    // semibold 12 px figure of about 7 px a digit and the plus.
    let plus = (count.to_string().len() + 1) as f32 * 7.0 + 8.0;
    INLINE.min(fit(width - plus))
}

/// The full profile's badges on the username's line: those that fit in a
/// pill, and "+n" for the rest, which shows them all under it.
fn inline_badges<'a>(
    badges: &'a Badges,
    place: Place,
    ink: Tone,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let line = responsive(move |room: iced::Size| {
        let count = badges.badges.len();
        let shown = shown(count, room.width);
        let mut row = Row::<Element<'a, people::Intent>>::new()
            .spacing(2)
            .align_y(Alignment::Center);
        for badge in &badges.badges[..shown] {
            row = row.push(one::view(badge, motion).map(people::Intent::from).boxed());
        }
        if shown < count {
            let plus = text(format!("+{}", count - shown))
                .size(PLUS_SIZE)
                .font(typography::SEMIBOLD)
                .style(move |theme: &Theme| text::Style {
                    color: Some(ink.at(theme.palette())),
                    ..text::Style::default()
                });
            row = row.push(
                button(container(plus).padding(Padding::from([0.0, 4.0])))
                    .padding(0)
                    .style(anomi_theme::catalog::button::bare)
                    .on_press(Intent::AllBadges(place).into())
                    .boxed(),
            );
        }
        let inside = container(row).padding(Padding {
            top: 1.0,
            bottom: 1.0,
            left: 3.0,
            right: 3.0,
        });
        container(pill::pill(inside, motion))
            .height(CHIP)
            .align_y(Alignment::Center)
            .boxed()
    });
    container(line)
        .width(Fill)
        .height(Length::Fixed(CHIP))
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::shown;

    #[test]
    fn badges_show_while_they_fit_and_six_at_most() {
        // 6 px of pill and 22 a badge: three fit in 72 px.
        assert_eq!(shown(3, 72.0), 3);
        assert_eq!(shown(6, 400.0), 6);
        // Nine leave room for "+3" after six.
        assert_eq!(shown(9, 400.0), 6);
        // Too narrow for all four: fewer, with "+n".
        assert_eq!(shown(4, 80.0), 2);
    }
}
