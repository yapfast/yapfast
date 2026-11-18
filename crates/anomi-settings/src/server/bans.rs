//! A server's Bans: the search, the page of banned people, and the pager.
//! Task X06's (docs/iced/tasks/settings.md). The Slint side: `BansSection`
//! (`ui/moderation.slint:193`); the ban's dialog is [`dialog`].

mod dialog;
mod rows;

use iced::widget::{column, container, row, text};
use iced::{Alignment, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::bans::{Bans, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use yapfast_lang::tr;

pub use dialog::dialog;

const GAP: f32 = 16.0;
const SEARCH: f32 = 8.0;
const FIELD: f32 = 36.0;
const PAGER: f32 = 12.0;
const EMPTY: f32 = 8.0;
const EMPTY_TITLE: f32 = 17.0;

pub fn view(bans: &Bans, motion: Motion) -> impl View<Intent> + '_ {
    let search = row![
        field(tr!("Search Bans by User Id or Username"), &bans.query)
            .on_input(Intent::Query)
            .on_submit(Intent::Search)
            .text_size(typography::size::SMALL)
            .height(FIELD)
            .motion(motion)
            .view(),
        action_button(tr!("Search"), Kind::Primary, motion)
            .enabled(!bans.fetching)
            .on_press(Intent::Search),
    ]
    .spacing(SEARCH)
    .height(FIELD);

    let mut page = column![
        text(tr!(
            "Bans by default are by account and IP.\nA user can circumvent an IP ban by using a proxy.\nBan circumvention can be made very hard by enabling phone verification in Safety Setup."
        ))
        .size(typography::size::LABEL)
        .style(catalog::text::muted),
        search,
    ]
    .spacing(GAP)
    .height(Length::Fill);

    if bans.rows.is_empty() {
        page = page.push(if bans.fetching {
            text(tr!("Loading..."))
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .boxed()
        } else {
            empty()
        });
    } else {
        page = page.push(rows::rows(bans, motion));
    }
    if bans.pages > 1 {
        page = page.push(pager(bans, motion));
    }
    page
}

/// Nobody banned: said in the middle of what is left.
fn empty<'a>() -> Element<'a, Intent> {
    container(
        column![
            text(tr!("No Bans"))
                .size(EMPTY_TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary),
            text(tr!(
                "You haven't banned anybody...\nbut if and when you must, do not hesitate!"
            ))
            .size(typography::size::SMALL)
            .align_x(Alignment::Center)
            .style(catalog::text::secondary),
        ]
        .spacing(EMPTY)
        .align_x(Alignment::Center),
    )
    .center(Length::Fill)
    .boxed()
}

/// Previous and Next around where the page is.
fn pager(bans: &Bans, motion: Motion) -> Element<'_, Intent> {
    let mut previous = action_button(tr!("Previous"), Kind::Ghost, motion).enabled(bans.page > 0);
    if bans.page > 0 {
        previous = previous.on_press(Intent::Page(bans.page - 1));
    }
    let more = bans.page + 1 < bans.pages;
    let mut next = action_button(tr!("Next"), Kind::Ghost, motion).enabled(more);
    if more {
        next = next.on_press(Intent::Page(bans.page + 1));
    }
    let label = tr!("Page {} of {}", bans.page + 1, bans.pages);
    container(
        row![
            previous,
            text(label)
                .size(typography::size::SMALL)
                .style(catalog::text::secondary),
            next,
        ]
        .spacing(PAGER)
        .align_y(Alignment::Center),
    )
    .center_x(Length::Fill)
    .boxed()
}

/// Its shots for `anomi-shots`: the list, and a ban open over it.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::bans::{dialog, page};
    let shot = |name: &'static str, vm: fn() -> anomi_vm::Vm| Shot {
        area: "ssettings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![shot("bans", page), shot("bans_open", dialog)]
}
