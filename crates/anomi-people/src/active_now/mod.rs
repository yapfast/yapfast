//! The Active Now column beside the friends list: a card for each friend or
//! party doing something, under the heading, with a quiet message when
//! nobody is. Times count from `ActiveNow::now`, which the app's clock moves
//! only while the window is in front. The Slint side: `ActiveNowColumn`
//! (`ui/active_now.slint:186`), filled by `src/ui/active_now.rs`.

mod card;
mod faces;
mod item;
mod lit;
#[cfg(feature = "shots")]
mod shots;
mod time;

use iced::font::Weight;
use iced::widget::text::LineHeight;
use iced::widget::{column, container, space, stack, text};
use iced::{Fill, Pixels, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::people::active_now::{ActiveNow, Intent};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The quiet message sits 8 px lower (`:211`) and its two lines 6 px apart.
const QUIET_TOP: f32 = 8.0;
const QUIET_GAP: f32 = 6.0;
/// The heading (`:206`), and the quiet message's two lines.
const HEADING: f32 = 20.0;
/// The heading's line box.
const HEADING_LINE: f32 = 27.0;
const QUIET_TITLE: f32 = 16.0;

pub fn view(active_now: &ActiveNow, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts = column![
        text(tr!("Active Now"))
            .size(HEADING)
            .font(typography::weight(Weight::ExtraBold))
            .line_height(LineHeight::Absolute(Pixels(HEADING_LINE)))
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
    ]
    .spacing(metrics::SPACE_L)
    .width(Fill)
    .height(Fill);
    if active_now.cards.is_empty() {
        parts = parts.push(quiet());
    } else {
        let cards = column(
            active_now
                .cards
                .iter()
                .map(|card| card::view(card, active_now.now, motion)),
        )
        .spacing(metrics::SPACE_M)
        .width(Fill);
        parts = parts.push(scroll_view(cards, motion).boxed());
    }
    // The 1 px rule lies over the panel, the padding counted from the
    // column's own edge (`:193`).
    let edge = container(space())
        .width(1)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().line.into()),
            ..container::Style::default()
        });
    let content = container(parts)
        .padding(metrics::SPACE_L)
        .width(Fill)
        .height(Fill);
    container(stack![content, edge])
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().panel.into()),
            ..container::Style::default()
        })
}

/// What shows while no friend is doing anything.
fn quiet<'a>() -> Element<'a, Intent> {
    container(
        column![
            text(tr!("It's quiet for now..."))
                .size(QUIET_TITLE)
                .font(typography::BOLD)
                .align_x(alignment::Horizontal::Center)
                .width(Fill)
                .style(catalog::text::primary),
            text(tr!(
                "When a friend starts an activity — like playing a game or hanging out on voice — we'll show it here!"
            ))
            .size(typography::size::LABEL)
            .align_x(alignment::Horizontal::Center)
            .width(Fill)
            .style(catalog::text::muted),
        ]
        .spacing(QUIET_GAP),
    )
    .padding(iced::Padding::ZERO.top(QUIET_TOP))
    .width(Fill)
    .boxed()
}
