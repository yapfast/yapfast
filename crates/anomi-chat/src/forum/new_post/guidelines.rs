//! The guidelines card: the forum's topic under a heading, which folds with
//! the chevron (`ui/forum.slint:298`-`:322`). Its text scrolls past 220 px.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::new_post::{Intent, NewPost};
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;
use anomi_widgets::list::scroll_view;
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

const RADIUS: f32 = 10.0;
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;
const HEADING: f32 = 14.0;
const TALLEST: f32 = 220.0;
const CHEVRON: f32 = 24.0;
const CHEVRON_GLYPH: f32 = 16.0;
/// The heading row's `spacing: 6px` (`ui/forum.slint:322`).
const HEAD_GAP: f32 = 6.0;

/// The card, or nothing where the forum has no guidelines.
pub(super) fn view<'a>(post: &'a NewPost, motion: Motion) -> Option<Element<'a, Intent>> {
    let guidelines = post.guidelines.as_ref()?;
    let (symbol, label) = if post.guidelines_open {
        (Icon::ChevronDown, tr!("Hide post guidelines"))
    } else {
        (Icon::ChevronRight, tr!("Show post guidelines"))
    };
    let head = row![
        text(tr!("Post Guidelines"))
            .size(HEADING)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::primary)
            .width(Fill),
        icon_button(symbol, label, motion)
            .size(CHEVRON)
            .glyph(CHEVRON_GLYPH)
            .on_press(Intent::ToggleGuidelines)
            .view(),
    ]
    .spacing(HEAD_GAP)
    .align_y(Alignment::Center);
    let mut parts = column![head].spacing(GAP);
    if post.guidelines_open {
        parts = parts.push(
            scroll_view(
                paragraph(guidelines)
                    .size(typography::size::SMALL)
                    .color(|palette| palette.secondary)
                    .motion(motion)
                    .on_link(Intent::OpenLink),
                motion,
            )
            .height(Length::Shrink.max(TALLEST))
            .boxed(),
        );
    }
    Some(
        container(parts.padding(PAD))
            .width(Fill)
            .style(|theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(palette.sidebar.into()),
                    border: Border::default()
                        .rounded(RADIUS)
                        .width(1)
                        .color(palette.line),
                    ..container::Style::default()
                }
            })
            .boxed(),
    )
}
