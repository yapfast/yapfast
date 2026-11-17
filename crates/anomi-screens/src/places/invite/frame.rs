//! What the five dialogs about a server's link, an event, a gift or an
//! application share: the raised card with its title and X, the small
//! capitals over a part, the wells, the found row and the buttons that end
//! it. The Slint side: the repeated body of `InviteDialog`, `JoinDialog`,
//! `EventDialog`, `GiftDialog` and `OAuthDialog` (`ui/server.slint:27`,
//! `:139`, `:237`, `:311`, `:395`). The layer around the card (its dimmed
//! backdrop, Escape, a press outside, the way in) is the shell's.

use std::cell::RefCell;
use std::collections::HashMap;

use anomi_theme::Renderer;
use iced::advanced::text::Renderer as TextRenderer;
use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{
    Alignment as TextAlignment, Ellipsis, LineHeight, Shaping, Text, Wrapping,
};
use iced::widget::{Id, column, container, row, text};
use iced::{Alignment, Background, Border, Length, Pixels, Shadow, Size, Vector, alignment};

use anomi_theme::metrics::{RADIUS_CARD, SPACE_M, SPACE_S, SPACE_XS};
use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Avatar;
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::{Kind, action_button, icon_button};
use yapfast_lang::tr;

/// The card's inner padding and the space between its parts (`:57`).
pub const PADDING: f32 = 20.0;
const SPACING: f32 = SPACE_M;
/// The title (`font-size: 18px`), and the X after it: its button and glyph.
const TITLE: f32 = 18.0;
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The card's shadow (`drop-shadow-blur: 32px`, `-offset-y: 12px`,
/// `#00000066`).
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_DROP: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;
/// A heading's line is one pixel shorter than iced's 1.3 of 12 px, the room
/// Slint's `Text` leaves above it.
const HEADING_LINE: f32 = 14.6;
/// The picture of a found server or application (`:186`).
const PICTURE: f32 = 48.0;
/// The found column's gap between the name and its lines (`spacing: 2px`,
/// `:189`).
const FOUND_SPACING: f32 = 2.0;
/// An `ActionButton`'s padding either side and its least width
/// (`ui/controls.slint:21`, `:51`), and the space between buttons (`:57`).
const BUTTON_PAD: f32 = 16.0;
const BUTTON_LEAST: f32 = metrics::CONTROL_HEIGHT;
/// A well that holds a line and a button (`:66`).
pub const WELL: f32 = 44.0;
/// The ids of the buttons every dialog has, to focus them from a test or a
/// task: the one that closes it (Close, Cancel, Done), its main one, and
/// the one that copies.
pub const CANCEL: &str = "dialog-cancel";
pub const CONFIRM: &str = "dialog-confirm";
pub const COPY: &str = "dialog-copy";

thread_local! {
    static WORDS: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
}

/// How wide the button saying `label` is: its bold words and its padding.
fn button_width(label: &str) -> f32 {
    if let Some(found) = WORDS.with(|cache| cache.borrow().get(label).copied()) {
        return found;
    }
    let text = Text {
        content: label.to_owned(),
        bounds: Size::new(f32::INFINITY, BUTTON_LEAST),
        size: Pixels(typography::size::BODY),
        line_height: LineHeight::default(),
        font: typography::BOLD,
        align_x: TextAlignment::Center,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::None,
        hint_factor: None,
    };
    let words = Plain::<<Renderer as TextRenderer>::Paragraph>::new(text)
        .min_width()
        .ceil();
    let width = (words + 2.0 * BUTTON_PAD).max(BUTTON_LEAST);
    WORDS.with(|cache| cache.borrow_mut().insert(label.to_owned(), width));
    width
}

/// The card's width: `least`, or more when the row of buttons saying
/// `labels` needs it (`width: min(max(440px, content.min-width),
/// root.width - 48px)`, `ui/server.slint:154`). The layer leaves 16 px of
/// the window (8 px each side), which a fixed width yields to.
pub fn fit(least: f32, labels: &[&str]) -> f32 {
    let row: f32 = labels.iter().map(|label| button_width(label)).sum::<f32>()
        + SPACE_S * labels.len().saturating_sub(1) as f32;
    least.max(row + 2.0 * PADDING)
}

/// What the first button says: Close when nothing more is on offer.
pub fn leave_label(only_close: bool) -> &'static str {
    if only_close {
        tr!("Close")
    } else {
        tr!("Cancel")
    }
}

/// The card: `title` and an X that sends `close` over `parts`, `width` wide
/// unless the window is narrower.
pub fn dialog<'a, M: Clone + 'a>(
    title: &'a str,
    width: f32,
    close: M,
    motion: Motion,
    parts: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let bar: Element<'a, M> = row![
        text(title)
            .size(TITLE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Length::Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(CLOSE)
            .glyph(CLOSE_GLYPH)
            .on_press(close)
            .view(),
    ]
    .align_y(Alignment::Center)
    .boxed();
    let mut body = column![bar].spacing(SPACING);
    for part in parts {
        body = body.push(part);
    }
    container(body)
        .padding(PADDING)
        .width(Length::Fixed(width))
        .style(card)
        .boxed()
}

/// Raised, edged, rounded by the dialog's radius, with its shadow
/// (`drop-shadow-blur: 32px`, `#00000066`, 12 px down).
fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: iced::Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// The small bold capitals over a part, such as "INVITE LINK".
pub fn heading<'a, M: 'a>(label: &'a str) -> Element<'a, M> {
    text(label)
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .height(HEADING_LINE)
        .align_y(iced::alignment::Vertical::Bottom)
        .style(catalog::text::muted)
        .boxed()
}

/// A grey line of explanation: `Label`.
pub fn note<'a, M: 'a>(label: &'a str) -> Element<'a, M> {
    typography::label(label).boxed()
}

/// Ordinary text that wraps.
pub fn body<'a, M: 'a>(label: &'a str) -> Element<'a, M> {
    text(label)
        .size(typography::size::SMALL)
        .style(catalog::text::primary)
        .boxed()
}

/// Why it failed, in red.
pub fn failure<'a, M: 'a>(label: &'a str) -> Element<'a, M> {
    text(label)
        .size(typography::size::SMALL)
        .style(catalog::text::danger)
        .boxed()
}

/// Red and bold: what the application can take over.
pub fn warning<'a, M: 'a>(label: &'a str) -> Element<'a, M> {
    text(label)
        .size(typography::size::SMALL)
        .font(typography::BOLD)
        .style(catalog::text::danger)
        .boxed()
}

/// A line in a well: one line, elided, in the ink's size.
pub fn line<'a, M: 'a>(label: &'a str, size: f32, muted: bool) -> Element<'a, M> {
    let line = text(label)
        .size(size)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Length::Fill);
    if muted {
        line.style(catalog::text::muted).boxed()
    } else {
        line.boxed()
    }
}

/// A sunken well `content` fills, with the rim a link's has when `rim`.
pub fn well<'a, M: 'a>(content: Element<'a, M>, height: Option<f32>, rim: bool) -> Element<'a, M> {
    let well = container(content)
        .width(Length::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let border = Border::default().rounded(RADIUS_CARD);
            container::Style {
                background: Some(Background::Color(palette.sunken)),
                border: if rim {
                    border.width(1).color(palette.line)
                } else {
                    border
                },
                ..container::Style::default()
            }
        });
    match height {
        Some(height) => well.height(height).center_y(height).boxed(),
        None => well.boxed(),
    }
}

/// A well with a line and a button at its end: a link to copy, with the rim
/// the invite link's has (`:63`) when `rim`; an address's has none (`:458`).
pub fn copyable<'a, M: Clone + 'a>(
    line_of: Element<'a, M>,
    rim: bool,
    copied: bool,
    enabled: bool,
    on_copy: M,
    motion: Motion,
) -> Element<'a, M> {
    let kind = if copied {
        Kind::Secondary
    } else {
        Kind::Primary
    };
    let label = if copied { tr!("Copied") } else { tr!("Copy") };
    let button = action_button(label, kind, motion)
        .id(Id::new(COPY))
        .enabled(enabled)
        .on_press(on_copy);
    let content: Element<'a, M> = row![line_of, button]
        .spacing(SPACE_S)
        .align_y(Alignment::Center)
        .padding(iced::Padding {
            left: SPACE_M,
            right: SPACE_XS,
            ..iced::Padding::ZERO
        })
        .boxed();
    well(content, Some(WELL), rim)
}

/// What was found: a name over its details in a well, with a picture before
/// them when there is one.
pub fn found<'a, M: 'a>(
    avatar: Option<&'a Avatar>,
    name: &'a str,
    lines: &[&'a str],
) -> Element<'a, M> {
    let mut text_column = column![
        text(name)
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Length::Fill)
    ]
    .spacing(FOUND_SPACING);
    for detail in lines.iter().filter(|detail| !detail.is_empty()) {
        text_column = text_column.push(note(detail));
    }
    let content: Element<'a, M> = match avatar {
        Some(avatar) => row![
            portrait(avatar, PICTURE).view(),
            container(text_column)
                .center_y(Length::Shrink)
                .width(Length::Fill)
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center)
        .padding(SPACING)
        .boxed(),
        None => container(text_column).padding(SPACING).boxed(),
    };
    well(content, None, false)
}

/// The buttons that end it, at the right.
pub fn buttons<'a, M: 'a>(buttons: Vec<Element<'a, M>>) -> Element<'a, M> {
    let mut line = row![].spacing(SPACE_S);
    for button in buttons {
        line = line.push(button);
    }
    container(line)
        .width(Length::Fill)
        .align_right(Length::Fill)
        .padding(iced::Padding {
            top: SPACE_XS,
            ..iced::Padding::ZERO
        })
        .boxed()
}

/// The first button: Close when nothing more is on offer, else Cancel.
pub fn leave<'a, M: Clone + 'a>(only_close: bool, close: M, motion: Motion) -> Element<'a, M> {
    action_button(leave_label(only_close), Kind::Ghost, motion)
        .id(Id::new(CANCEL))
        .on_press(close)
        .boxed()
}

/// The main button, in the accent; it waits while `busy`.
pub fn confirm<'a, M: Clone + 'a>(
    label: &'a str,
    busy: bool,
    on_press: M,
    motion: Motion,
) -> Element<'a, M> {
    action_button(label, Kind::Primary, motion)
        .id(Id::new(CONFIRM))
        .enabled(!busy)
        .on_press(on_press)
        .boxed()
}
