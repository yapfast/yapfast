//! The files waiting to go with the next message: `StagedTile`
//! (`ui/composer.slint:12`) and the row of them above the field
//! (`ui/yapfast.slint:4180`), which scrolls sideways.
//!
//! A tile is 120 by 140: the file's picture (blurred under a SPOILER cover
//! while it is one), or a file icon where it is none, its name and its size
//! under it, and over the picture's top right corner two buttons on dark
//! discs: cover or uncover it, where the room has spoilers, and take it
//! out. The Slint tile fades in as it is staged; iced at this pin has no
//! layer opacity, so it appears at once.

use iced::widget::{column, container, row, scrollable, stack, svg, text};
use iced::{Color, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::composer::{Intent, Staged};
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;
use anomi_widgets::cover::spoiler_cover;
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{self, Fit};

/// `width: 120px; height: 140px; border-radius: 10px`.
const WIDTH: f32 = 120.0;
const HEIGHT: f32 = 140.0;
const RADIUS: f32 = 10.0;
/// `pad: 6px`: the room around the picture, which its radius takes off the
/// tile's.
const PAD: f32 = 6.0;
/// The picture's radius.
const INNER: f32 = RADIUS - PAD;
/// The picture's height.
const PICTURE: f32 = 86.0;
/// The two buttons' discs (`24px`), the glyph in them, and the gap to the
/// picture's edge and between them (`4px`).
const DISC: f32 = 24.0;
const GLYPH: f32 = 14.0;
const GAP: f32 = 4.0;
/// A file without a picture draws this icon, 32 px.
const FILE: f32 = 32.0;
/// The row's room: `padding: 10px; padding-bottom: 2px; spacing: 8px`.
const ROW_PAD: f32 = 10.0;
const ROW_BOTTOM: f32 = 2.0;
const ROW_GAP: f32 = 8.0;
/// The row's height: the tile and its padding (`height: 152px`).
pub const ROW_HEIGHT: f32 = HEIGHT + ROW_PAD + ROW_BOTTOM;
/// The discs: `#000000b3`.
const DISC_GROUND: Color = Color::from_rgba(0.0, 0.0, 0.0, 0xb3 as f32 / 255.0);

/// The row of staged files, scrolling sideways when they overflow. The
/// thread's and the new post's composers draw it too (`StagedTile` is shared).
pub fn files<'a>(files: &'a [Staged], can_spoil: bool, motion: Motion) -> Element<'a, Intent> {
    let tiles = iced::widget::Row::with_children(
        files
            .iter()
            .enumerate()
            .map(|(index, file)| tile(index, file, can_spoil, motion)),
    )
    .spacing(ROW_GAP)
    .padding(iced::Padding {
        top: ROW_PAD,
        right: ROW_PAD,
        bottom: ROW_BOTTOM,
        left: ROW_PAD,
    });
    scrollable(tiles)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .smooth_scroll(motion.playing())
        .boxed()
}

/// One staged file at `index`.
fn tile<'a>(
    index: usize,
    file: &'a Staged,
    can_spoil: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let inner = INNER;
    let shown = match &file.picture {
        Some(handle) => {
            // A spoiler's picture is blurred data under its cover.
            let handle = if file.spoiler {
                picture::blurred(handle).unwrap_or_else(|| handle.clone())
            } else {
                handle.clone()
            };
            picture::picture(handle)
                .width(Length::Fill)
                .height(PICTURE)
                .radius(inner)
                .fit(Fit::Cover)
                .motion(motion)
                .boxed()
        }
        None => container(icon(Icon::File, FILE).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().muted),
        }))
        .center(Length::Fill)
        .style(ground)
        .boxed(),
    };
    let covered = if file.spoiler {
        stack![shown, spoiler_cover(inner)].boxed()
    } else {
        shown
    };
    let picture = container(covered)
        .width(Length::Fill)
        .height(PICTURE)
        .clip(true)
        .style(ground);
    let body = container(
        column![
            picture,
            text(file.name.as_str())
                .size(typography::size::LABEL)
                .font(typography::MEDIUM)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .height(Length::Fill)
                .style(catalog::text::primary),
            text(file.size.as_str())
                .size(typography::size::CAPTION)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .height(Length::Fill)
                .style(catalog::text::muted),
        ]
        .spacing(2)
        .width(Length::Fill),
    )
    .padding(PAD)
    .width(WIDTH)
    .height(HEIGHT)
    .style(|theme: &Theme| iced::widget::container::Style {
        background: Some(theme.palette().raised.into()),
        border: iced::Border::default().rounded(RADIUS),
        ..Default::default()
    });
    let mut buttons = row![].spacing(GAP);
    if can_spoil {
        let symbol = if file.spoiler {
            Icon::EyeOff
        } else {
            Icon::Eye
        };
        let intent = Intent::ToggleSpoiler(index);
        buttons = buttons.push(disc(symbol, &file.spoil_label, intent, motion));
    }
    buttons = buttons.push(disc(
        Icon::X,
        &file.remove_label,
        Intent::Unstage(index),
        motion,
    ));
    // Inside the picture, 4 px from its edges.
    let over = container(buttons)
        .padding(iced::Padding {
            top: PAD + GAP,
            right: PAD + GAP,
            bottom: 0.0,
            left: 0.0,
        })
        .width(WIDTH)
        .height(HEIGHT)
        .align_x(alignment::Horizontal::Right);
    stack![body, over].width(WIDTH).height(HEIGHT).boxed()
}

/// The picture's ground, rounded as the picture is.
fn ground(theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme.palette().sidebar.into()),
        border: iced::Border::default().rounded(INNER),
        ..Default::default()
    }
}

/// A button on a dark disc, white, that reads on any picture.
fn disc<'a>(symbol: Icon, label: &'a str, intent: Intent, motion: Motion) -> Element<'a, Intent> {
    icon_button(symbol, label, motion)
        .size(DISC)
        .glyph(GLYPH)
        .radius(DISC / 2.0)
        .tint(|_| Color::WHITE)
        .fill(|_| DISC_GROUND)
        .on_press(intent)
        .view()
        .boxed()
}
