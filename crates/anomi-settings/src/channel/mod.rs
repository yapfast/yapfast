//! A channel's settings frame and its overview. The Slint side:
//! `ChannelSettingsScreen` (`ui/channel_settings.slint:466`) over what
//! `show_channel_settings` fills (`src/ui/channel_settings.rs`): the settings
//! frame ([`crate::frame::pane`]) with the channel's sections on its left
//! (its name in capitals over Overview and Permissions, and Delete Channel in
//! red after a line) and the page on its right, the overview
//! ([`overview`]) or X14's permissions, which take the frame's height as a
//! list with its own scroll and a wide column. The bar for unsaved changes
//! floats while the draft differs or is on its way; what Discord refused and
//! why the draft can't be saved are in red under the page.
//! Escape and a press outside close it: the layer's.

pub mod create;
mod overview;
pub mod permissions;

use iced::widget::{column, container, text};
use iced::{Fill, Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings;
use anomi_vm::settings::channel::{Channel, Intent, Page, Row};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use crate::frame::pane;
use crate::kit::{heading, nav_row};

/// The sections' width, insets and rows, as the kit's `sections` has them
/// (`ui/settings.slint:368`); it can't be used as it borrows its rows.
const WIDTH: f32 = 266.0;
const SIDES: f32 = 16.0;
const TOP: f32 = 12.0;
const BOTTOM: f32 = 16.0;
const HEADING_INSET: f32 = 8.0;
const HEADING_HEIGHT: f32 = 24.0;
const LINE_SIDES: f32 = 8.0;
const LINE_ABOVE: f32 = 12.0;

pub fn view(channel: &Channel, motion: Motion) -> impl View<settings::Intent> + '_ {
    let intent = |intent: Intent| settings::Intent::from(intent);
    let permissions = channel.page == Page::Permissions;
    pane(
        move || {
            sections(channel, motion)
                .map(settings::Intent::from)
                .boxed()
        },
        move || body(channel, motion),
        &channel.title,
        &channel.close,
        intent(Intent::Close),
        motion,
    )
    .key(channel.page as u64)
    .wide(permissions)
    .fill(permissions && !channel.loading)
    .unsaved(
        channel.unsaved.as_ref(),
        intent(Intent::Undo),
        intent(Intent::Save),
    )
    .view()
}

/// The sections: the heading, a page per row, the line before the red one.
fn sections(channel: &Channel, motion: Motion) -> Element<'_, Intent> {
    let mut entries: Vec<Element<'_, Intent>> = Vec::with_capacity(channel.rows.len());
    for (index, entry) in channel.rows.iter().enumerate() {
        entries.push(match entry {
            Row::Heading(words) => container(heading(words.as_str()))
                .padding(Padding {
                    left: HEADING_INSET,
                    top: if index == 0 { 0.0 } else { 16.0 },
                    ..Padding::ZERO
                })
                .height(if index == 0 { HEADING_HEIGHT } else { 40.0 })
                .align_y(alignment::Vertical::Center)
                .boxed(),
            Row::Line => container(iced::widget::rule::horizontal(1).style(catalog::rule::line))
                .padding(Padding {
                    top: LINE_ABOVE,
                    bottom: LINE_ABOVE,
                    left: LINE_SIDES,
                    right: LINE_SIDES,
                })
                .boxed(),
            Row::Page {
                text,
                icon,
                page,
                danger,
            } => nav_row(
                *icon,
                text.as_str(),
                *page == channel.page,
                *danger,
                false,
                Intent::Choose(*page),
                motion,
            ),
        });
    }
    let list = column(entries).padding(Padding {
        top: TOP,
        bottom: BOTTOM,
        left: SIDES,
        right: SIDES,
    });
    container(scroll_view(list, motion))
        .width(Length::Fixed(WIDTH))
        .height(Fill)
        .style(crate::kit::chat_ground)
        .boxed()
}

/// The page: loading, the overview or the permissions, then what is wrong.
fn body(channel: &Channel, motion: Motion) -> Element<'_, settings::Intent> {
    let mut parts: Vec<Element<'_, settings::Intent>> = Vec::new();
    if channel.loading {
        parts.push(
            text(tr!("Loading..."))
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .boxed(),
        );
    } else if channel.page == Page::Permissions {
        parts.push(
            permissions::view(&channel.permissions, motion)
                .boxed()
                .map(settings::Intent::from)
                .boxed(),
        );
    } else {
        parts.push(
            overview::view(channel, motion)
                .boxed()
                .map(settings::Intent::from)
                .boxed(),
        );
    }
    for wrong in [&channel.problem, &channel.error] {
        if !wrong.is_empty() {
            parts.push(
                text(wrong.as_str())
                    .size(typography::size::SMALL)
                    .style(catalog::text::danger)
                    .width(Fill)
                    .boxed(),
            );
        }
    }
    column(parts).spacing(16).width(Fill).boxed()
}

/// Its shots for `anomi-shots`: the golden screenshot of the overview, and
/// the others of its pages for the Slint app's side by side
/// (`scripts/ui-session`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::channel as sample;

    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "csettings",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("overview", || sample::asked(sample::overview())),
        shot("overview_changed", || sample::asked(sample::changed())),
        shot("overview_voice", || sample::asked(sample::voice())),
        shot("overview_category", || sample::asked(sample::category())),
        shot("overview_loading", || sample::asked(sample::loading())),
        shot("overview_refused", || sample::asked(sample::refused())),
    ]
}
