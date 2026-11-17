//! A tile's name at its bottom left (`TileChip`, `ui/voice.slint:819`): on
//! a dark scrim with 6 px corners, 26 px tall, 8 px inside its ends, 6 px
//! between its parts: the mark, the name in white, 14 px bold, the LIVE tag
//! and the note in the accent, 13 px. As wide as they are, at most the
//! tile's width less its insets. Where that is too narrow the mark and the
//! LIVE tag keep their width and the name and the note share what is left,
//! each cut short with an ellipsis: `Fluid(Max)` lays them out after the
//! fixed parts, each no wider than it is, the name first at no more than
//! half the room when the note is there too.

use iced::widget::{Row, container, text};
use iced::{Alignment, Background, Border, Color, Length, Padding};
use iced_runtime::core::length::Constraint;

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::voice::tiles::{Chip, Intent, Mark};
use anomi_widgets::icon::{Icon, icon};

use super::{SCRIM, live::live};

const HEIGHT: f32 = 26.0;
const RADIUS: f32 = 6.0;
const SIDE: f32 = 8.0;
const SPACING: f32 = 6.0;
const NAME: f32 = typography::size::SMALL;
const NOTE: f32 = typography::size::HEADING;
/// The mark before the name (`:1113`, `:1114`, `:1048`).
const MARK: f32 = 14.0;
/// How wide the name and the note are: what the fixed parts leave, never
/// wider than they are.
const SHRINKS: Length = Length::Fluid(Constraint::Max);

pub fn view(chip: &Chip) -> Element<'_, Intent> {
    let mut parts: Row<Element<'_, Intent>> = Row::new()
        .spacing(SPACING)
        .align_y(Alignment::Center)
        .height(HEIGHT);
    if let Some(mark) = chip.mark {
        parts = parts.push(mark_icon(mark));
    }
    parts = parts.push(
        text(chip.name.as_str())
            .size(NAME)
            .font(typography::BOLD)
            .color(Color::WHITE)
            .width(SHRINKS)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed(),
    );
    if chip.live {
        parts = parts.push(live(None));
    }
    if !chip.note.is_empty() {
        parts = parts.push(
            text(chip.note.as_str())
                .size(NOTE)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().accent),
                    ..text::Style::default()
                })
                .width(SHRINKS)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed(),
        );
    }
    container(parts)
        .padding(Padding::ZERO.left(SIDE).right(SIDE))
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(SCRIM)),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// Muted and deafened in red, a stream's screen in white.
fn mark_icon(mark: Mark) -> Element<'static, Intent> {
    let (shape, red) = match mark {
        Mark::Muted => (Icon::MicOff, true),
        Mark::Deafened => (Icon::HeadphoneOff, true),
        Mark::Screen => (Icon::ScreenShare, false),
    };
    icon(shape, MARK)
        .style(move |theme: &Theme, _status| iced::widget::svg::Style {
            color: Some(if red {
                theme.palette().danger
            } else {
                Color::WHITE
            }),
        })
        .boxed()
}
