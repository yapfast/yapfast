//! What a message says of itself: a notice's icon in place of the picture,
//! and the lines round its text (a forward, an app thinking, "(edited)", a
//! hidden card, a bot control that failed, its thread, its delivery, an
//! answer only this account sees). The Slint side: the notice's `Image`
//! and those lines of `MessageContent` (`ui/messages.slint:3097`-`:3105`,
//! `:3264`-`:3282`, `:3300`, `:3323`-`:3365`) and `ThreadChip` (`:1989`).

use iced::font::{self, Font};
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{button, container, row, space, svg, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, catalog, typography};
use anomi_vm::chat::message::{Delivery, Intent, Message, Notice, Thinking, Thread};
use anomi_vm::chat::{self};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// A notice's icon: 16 px, 12 px in (none in the compact display), level
/// with the text's first line (`:3098`).
const NOTICE: f32 = 16.0;
const NOTICE_X: f32 = 12.0;
/// The lines' icons are 16 px, 4 px before their text (`:3265`, `:3350`).
const GLYPH: f32 = 16.0;
const GLYPH_GAP: f32 = 4.0;
/// "Forwarded", the delivery and "(edited)": 14, 13 and 12 px (`:3267`,
/// `:3344`, `:3282`); an ephemeral answer's note 12; a hidden card's 7/8 of
/// the text's size.
const FORWARDED: f32 = 14.0;
const DELIVERY: f32 = 13.0;
const SMALL: f32 = 12.0;
const HIDDEN_SCALE: f32 = 0.875;
/// Why a bot control failed: 4 px under the controls, the icon 6 px before
/// 14 px text (`:3324`-`:3326`).
const FAILED_TOP: f32 = 4.0;
const FAILED_GAP: f32 = 6.0;
/// The thread's chip: 4 px under what's above, at most 432 px wide, 8 px
/// round, 8 by 12 px in, its parts 10 px apart; its icon 18 px, the arrow
/// 16, the count 14 (`ThreadChip`).
const CHIP_TOP: f32 = 4.0;
const CHIP_WIDTH: f32 = 432.0;
const CHIP_RADIUS: f32 = 8.0;
const CHIP_PADDING: [f32; 2] = [8.0, 12.0];
const CHIP_GAP: f32 = 10.0;
const CHIP_ICON: f32 = 18.0;
const CHIP_DETAIL: f32 = 14.0;

fn ask(intent: Intent) -> chat::Intent {
    chat::Intent::from(intent)
}

fn italic() -> Font {
    Font {
        style: font::Style::Italic,
        ..typography::UI
    }
}

/// `kind` drawn in `colour`.
fn glyph<'a>(kind: Icon, side: f32, colour: fn(&Palette) -> Color) -> svg::Svg<'a, Theme> {
    icon(kind, side).style(move |theme: &Theme, _status| svg::Style {
        color: Some(colour(theme.palette())),
    })
}

/// A notice's icon, in the gutter of a message of text `size`: live green
/// for a join or a call, muted otherwise.
pub fn icon_of<'a>(
    kind: Notice,
    size: f32,
    compact: bool,
    width: f32,
) -> Element<'a, chat::Intent> {
    let colour: fn(&Palette) -> Color = if kind.live() {
        |palette| palette.live
    } else {
        |palette| palette.muted
    };
    let left = if compact { 0.0 } else { NOTICE_X };
    let top = (size * 1.375 - NOTICE) / 2.0 + 1.0;
    container(glyph(kind.icon(), NOTICE, colour))
        .padding(Padding::default().top(top).left(left))
        .width(width)
        .boxed()
}

/// "Forwarded", as Discord's client marks a forward over what it carries.
pub fn forwarded<'a>() -> Element<'a, chat::Intent> {
    row![
        glyph(Icon::Forward, GLYPH, |palette| palette.muted),
        text(tr!("Forwarded"))
            .size(FORWARDED)
            .font(italic())
            .style(catalog::text::muted),
    ]
    .spacing(GLYPH_GAP)
    .align_y(Alignment::Center)
    .boxed()
}

/// An app's answer still to come, as Discord's client words it.
pub fn thinking(message: &Message, thinking: Thinking) -> Element<'_, chat::Intent> {
    let size = message.size.px();
    match thinking {
        Thinking::Waiting => text(tr!("{} is thinking…", message.author.name))
            .size(size)
            .font(italic())
            .style(catalog::text::muted)
            .boxed(),
        Thinking::TimedOut => text(tr!("The application did not respond"))
            .size(size)
            .style(catalog::text::danger)
            .boxed(),
    }
}

/// "(edited)" under an edited message.
pub fn edited<'a>() -> Element<'a, chat::Intent> {
    text(tr!("(edited)"))
        .size(SMALL)
        .style(catalog::text::muted)
        .boxed()
}

/// That a card is hidden, for a message that is only one.
pub fn hidden(note: &str, size: f32) -> Element<'_, chat::Intent> {
    text(note)
        .size(size * HIDDEN_SCALE)
        .font(italic())
        .style(catalog::text::muted)
        .boxed()
}

/// Why the last bot control used failed, under the controls.
pub fn failed(why: &str) -> Element<'_, chat::Intent> {
    row![
        glyph(Icon::TriangleAlert, GLYPH, |palette| palette.danger),
        text(why)
            .size(FORWARDED)
            .style(catalog::text::danger)
            .width(Fill),
    ]
    .spacing(FAILED_GAP)
    .padding(Padding::default().top(FAILED_TOP))
    .boxed()
}

/// The thread the message started, which a click opens beside the channel.
pub fn thread(key: u64, thread: &Thread) -> Element<'_, chat::Intent> {
    let mut chip = row![
        glyph(Icon::Thread, CHIP_ICON, |palette| palette.muted),
        text(thread.name.as_str())
            .font(typography::BOLD)
            .wrapping(Wrapping::None)
            .ellipsis(Ellipsis::End)
            .style(catalog::text::primary),
    ]
    .spacing(CHIP_GAP)
    .padding(CHIP_PADDING)
    .align_y(Alignment::Center);
    chip = if thread.detail.is_empty() {
        chip.push(space().width(Fill).boxed())
    } else {
        chip.push(
            text(thread.detail.as_str())
                .size(CHIP_DETAIL)
                .style(catalog::text::link)
                .width(Fill)
                .boxed(),
        )
    };
    chip = chip.push(glyph(Icon::ChevronRight, GLYPH, |palette| palette.muted).boxed());
    let pressed = button(chip)
        .padding(0)
        .width(Fill)
        .on_press(ask(Intent::Thread(key)))
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            let lifted = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if lifted {
                    palette.hover
                } else {
                    palette.sidebar
                })),
                border: Border {
                    color: palette.edge,
                    width: 1.0,
                    radius: CHIP_RADIUS.into(),
                },
                ..button::Style::default()
            }
        });
    // A fixed width gives way to a narrower row: at most 432 px.
    container(pressed)
        .width(CHIP_WIDTH)
        .padding(Padding::default().top(CHIP_TOP))
        .boxed()
}

/// A message on its way, or one that didn't go: what's happening, with
/// Retry and Cancel where they apply.
pub fn delivery(key: u64, delivery: &Delivery, motion: Motion) -> Element<'_, chat::Intent> {
    let failed = delivery.failed;
    let mut parts =
        row![
            text(delivery.text.as_str())
                .size(DELIVERY)
                .style(move |theme: &Theme| text::Style {
                    color: Some(if failed {
                        theme.palette().danger
                    } else {
                        theme.palette().muted
                    }),
                    ..text::Style::default()
                })
        ]
        .spacing(GLYPH_GAP)
        .align_y(Alignment::Center);
    if delivery.retry {
        let retry = action_button(tr!("Retry"), Kind::Ghost, motion).small();
        parts = parts.push(retry.on_press(ask(Intent::Retry(key))).boxed());
    }
    if delivery.cancel {
        let cancel = action_button(tr!("Cancel"), Kind::Ghost, motion).small();
        parts = parts.push(cancel.on_press(ask(Intent::CancelSend(key))).boxed());
    }
    parts.boxed()
}

/// "Only you can see this · Dismiss message", under an app's answer only
/// this account sees; Dismiss does nothing in the preview.
pub fn ephemeral(key: u64, preview: bool) -> Element<'static, chat::Intent> {
    let dismiss = button(text(tr!("Dismiss message")).size(SMALL))
        .padding(0)
        .on_press_maybe((!preview).then(|| ask(Intent::Dismiss(key))))
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                text_color: match status {
                    button::Status::Disabled => palette.muted,
                    button::Status::Hovered | button::Status::Pressed => palette.text,
                    _ => palette.link,
                },
                ..button::Style::default()
            }
        });
    row![
        glyph(Icon::Eye, GLYPH, |palette| palette.muted),
        text(tr!("Only you can see this ·"))
            .size(SMALL)
            .style(catalog::text::muted),
        dismiss,
    ]
    .spacing(GLYPH_GAP)
    .align_y(Alignment::Center)
    .boxed()
}
