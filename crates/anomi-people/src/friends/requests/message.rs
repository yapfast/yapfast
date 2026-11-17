//! A message request's latest message, under who sent it: clicking it opens
//! the conversation, which answers the request. The Slint side:
//! `RequestMessage` (`ui/friends.slint:21`). Unread, it is marked along its
//! edge and carries a New badge, as a new message is in the timeline.

use iced::font::Style as Italic;
use iced::widget::{button, container, hover, row, space, stack, text};
use iced::{Alignment, Background, Border, Fill, Font, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::people::requests::{Intent, Message};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

/// The corners, the padding and the gaps (`ui/friends.slint:33`, `:50`-`:51`).
const RADIUS: f32 = 8.0;
const PAD: f32 = 10.0;
const PAD_LEFT: f32 = 12.0;
const GAP: f32 = 10.0;
/// The speech bubble, and its drop below the padding (`:55`).
const GLYPH: f32 = 16.0;
const GLYPH_DROP: f32 = 2.0;
/// The words: 14 px, two lines at most (`:60`).
const WORDS: f32 = typography::size::SMALL;
const LINES: usize = 2;
/// The edge's mark when unread (`:48`).
const EDGE: f32 = 3.0;
/// What stands on the right: the New badge, the time and the chevron (`:69`).
const RIGHT_GAP: f32 = 6.0;
const NEW_HEIGHT: f32 = 16.0;
const NEW_DROP: f32 = 2.0;
const NEW_RADIUS: f32 = 4.0;
const NEW_PAD: f32 = 5.0;
const NEW_TEXT: f32 = 11.0;
const TIME: f32 = typography::size::CAPTION;

/// `from`'s `message`, which asks `Intent::Open` of its room.
/// `hovering` is the row's: only a hovered row builds the hovered look, the
/// resting one stays alone under it.
pub fn view<'a>(message: &'a Message, hovering: bool, motion: Motion) -> Element<'a, Intent> {
    let look = move |hovered: bool| {
        let content = content(message, hovered, motion);
        let face: Element<'a, Intent> = if hovered {
            button(content)
                .padding(0)
                .width(Fill)
                .on_press(Intent::Open(message.room.clone()))
                .style(move |theme: &Theme, _| button::Style {
                    background: Some(Background::Color(theme.palette().active)),
                    border: Border::default().rounded(RADIUS),
                    ..button::Style::default()
                })
                .boxed()
        } else {
            container(content)
                .width(Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(Background::Color(theme.palette().raised)),
                    border: Border::default().rounded(RADIUS),
                    ..container::Style::default()
                })
                .boxed()
        };
        if message.unread {
            stack![face, edge()].boxed()
        } else {
            face
        }
    };
    if hovering {
        hover(look(false), look(true)).boxed()
    } else {
        look(false)
    }
}

/// The mark along the left edge of an unread message.
fn edge<'a>() -> Element<'a, Intent> {
    container(space())
        .width(EDGE)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().accent)),
            border: Border {
                radius: iced::border::left(RADIUS),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .boxed()
}

fn content<'a>(message: &'a Message, hovered: bool, motion: Motion) -> Element<'a, Intent> {
    let unread = message.unread;
    let bubble =
        icon(Icon::MessageCircle, GLYPH).style(move |theme: &Theme, _| iced::widget::svg::Style {
            color: Some(if unread {
                theme.palette().accent
            } else {
                theme.palette().muted
            }),
        });
    let words: Element<'a, Intent> = if message.text.is_empty() {
        text(tr!("Open to read their message"))
            .size(WORDS)
            .font(Font {
                style: Italic::Italic,
                ..typography::UI
            })
            .style(catalog::text::muted)
            .boxed()
    } else {
        paragraph(&message.text)
            .size(WORDS)
            .max_lines(LINES)
            .selectable(false)
            .motion(motion)
            .color(move |palette| {
                if unread {
                    palette.text
                } else {
                    palette.secondary
                }
            })
            .boxed()
    };
    let mut right: iced::widget::Row<Element<'a, Intent>> =
        row![].spacing(RIGHT_GAP).align_y(Alignment::Center);
    if !message.time.is_empty() {
        right = right.push(
            text(message.time.as_str())
                .size(TIME)
                .wrapping(text::Wrapping::None)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    right = right.push(
        icon(Icon::ChevronRight, GLYPH)
            .style(move |theme: &Theme, _| iced::widget::svg::Style {
                color: Some(if hovered {
                    theme.palette().text
                } else {
                    theme.palette().muted
                }),
            })
            .boxed(),
    );
    let mut tail: iced::widget::Row<Element<'a, Intent>> =
        row![].spacing(RIGHT_GAP).align_y(Alignment::Start);
    if unread {
        tail = tail.push(
            container(new())
                .padding(Padding::ZERO.top(NEW_DROP))
                .boxed(),
        );
    }
    let tail = tail.push(right.boxed());
    row![
        container(bubble).padding(Padding::ZERO.top(GLYPH_DROP)),
        container(words).width(Fill),
        // The badge sits 2 px below the row's top, on whole pixels; the
        // time and the chevron stay centred in their own row.
        tail,
    ]
    .spacing(GAP)
    .padding(Padding::new(PAD).left(PAD_LEFT))
    .width(Fill)
    .boxed()
}

/// The "New" badge (`:64`): its own, 11 px bold, not the settings' NEW tag.
fn new<'a>() -> Element<'a, Intent> {
    container(
        text(tr!("New"))
            .size(NEW_TEXT)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().on_accent),
                ..text::Style::default()
            }),
    )
    .padding([0.0, NEW_PAD])
    .height(NEW_HEIGHT)
    .align_y(Alignment::Center)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().accent)),
        border: Border::default().rounded(NEW_RADIUS),
        ..container::Style::default()
    })
    .boxed()
}
