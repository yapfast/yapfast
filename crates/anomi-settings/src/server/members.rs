//! A server's Members settings: the server's members, newest first or in an
//! order chosen, a page at a time, found by name or id. A row opens its
//! member's profile and its More Options their menu, as Discord's page has
//! them. Task X03's (docs/iced/tasks/settings.md). The Slint side:
//! `MembersSection` (`ui/server_people.slint:117`).
//!
//! The table takes the frame's height (`fill`): the rows are a
//! [`list`](anomi_widgets::list::list), so thousands of members cost the
//! rows in view. The columns share the width as Discord's grow, and the
//! names above them start on the column's edge.

mod rows;

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::members::{Intent, Members, ONE_PAGE};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::list::list;
use yapfast_lang::{text_in, tr, trn};

use crate::kit::{self, picker, words};

/// Whether Members' table is narrow in a window of this size, so Joined
/// Discord goes (`narrow: frame.content-width < 720px`): the card's width
/// less the sections and the page's sides.
pub fn narrow(window: iced::Size) -> bool {
    use crate::frame::bar::{SIDES, WIDE};
    let card = kit::card::size(window).width;
    (card - kit::WIDTH - 2.0 * SIDES).min(WIDE) < 720.0
}
use rows::{AGE, JOIN, MORE, NAME, ROLES, SIGNALS, SINCE, cell};

const GAP: f32 = 16.0;
const SPACING: f32 = 12.0;
const SEARCH: f32 = 240.0;
const CONTROL: f32 = 36.0;
const SIZES: f32 = 80.0;

/// A quiet line of the page: the indexing, the loading.
fn label(words: &str) -> iced::widget::Text<'_, Theme> {
    text(words)
        .size(typography::size::LABEL)
        .style(catalog::text::muted)
        .width(Length::Fill)
}

/// A column's name over its column.
fn column_name<'a>(share: u16, words: &'a str) -> Element<'a, Intent> {
    cell(
        share,
        text(words)
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .style(catalog::text::secondary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    )
}

/// The title with the search and the order.
fn top<'a>(page: &'a Members, motion: Motion) -> Element<'a, Intent> {
    let title = if page.searched {
        tr!("Search Results")
    } else {
        tr!("Recent Members")
    };
    let sorts = words(&[
        tr!("Member Since (Newest first)"),
        tr!("Member Since (Oldest first)"),
        tr!("Joined Discord (Newest first)"),
        tr!("Joined Discord (Oldest first)"),
    ]);
    row![
        container(
            text(title)
                .size(typography::size::TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
        )
        .width(Length::Fill)
        .align_y(alignment::Vertical::Center)
        .height(CONTROL),
        container(
            field(tr!("Search by username or id"), page.search.as_str())
                .on_input(Intent::Search)
                .on_submit(Intent::Find(page.search.clone()))
                .text_size(typography::size::SMALL)
                .height(CONTROL)
                .motion(motion)
                .view()
        )
        .width(SEARCH),
        container(picker(sorts, page.sort, Some(Intent::Sort))).width(SEARCH),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center)
    .boxed()
}

/// The columns' names, laid as the rows' columns are.
fn names(page: &Members) -> Element<'_, Intent> {
    let mut names = vec![
        column_name(NAME, text_in("MembersSection", "NAME")),
        column_name(SINCE, tr!("MEMBER SINCE")),
    ];
    if !page.narrow {
        names.push(column_name(AGE, tr!("JOINED DISCORD")));
    }
    if page.join_method {
        names.push(column_name(JOIN, tr!("JOIN METHOD")));
    }
    names.push(column_name(ROLES, tr!("ROLES")));
    names.push(column_name(SIGNALS, tr!("SIGNALS")));
    names.push(space().width(MORE).boxed());
    row(names).spacing(rows::GAP).boxed()
}

/// The rows, reaching past the page's edges, or what stands in for them.
fn table<'a>(page: &'a Members, motion: Motion) -> Element<'a, Intent> {
    if page.rows.is_empty() {
        return if page.fetching {
            let loading = (!page.indexing).then(|| label(tr!("Loading...")));
            container(loading).height(Length::Fill).boxed()
        } else {
            container(
                text(tr!("No members match these search results."))
                    .size(typography::size::BODY)
                    .style(catalog::text::secondary)
                    .align_x(alignment::Horizontal::Center)
                    .width(Length::Fill),
            )
            .height(Length::Fill)
            .center_y(Length::Fill)
            .boxed()
        };
    }
    let members = &page.rows;
    kit::reach(
        rows::OUTSET,
        list(
            members.len(),
            |index| members[index].id.parse::<u64>().unwrap_or(index as u64),
            move |index| rows::member(&members[index], page, motion),
            motion,
        )
        .estimate(rows::HEIGHT)
        .height(Length::Fill),
    )
}

/// How many a page, of how many, and the pager while there is more than a
/// page.
fn footer<'a>(page: &'a Members, motion: Motion) -> Element<'a, Intent> {
    let mut line = row![].spacing(8).align_y(Alignment::Center);
    let many = page.total > ONE_PAGE;
    if many {
        let sizes = words(&["12", "25", "50", "100"]);
        line = line
            .push(
                text(tr!("Showing"))
                    .size(typography::size::SMALL)
                    .style(catalog::text::secondary)
                    .boxed(),
            )
            .push(
                container(picker(sizes, page.page_size, Some(Intent::PageSize)))
                    .width(SIZES)
                    .boxed(),
            );
    }
    let count = if many {
        tr!("members of {}", page.count)
    } else {
        trn!(
            "Showing {n} member",
            "Showing {n} members",
            page.total as u64
        )
    };
    line = line.push(
        container(
            text(count)
                .size(typography::size::SMALL)
                .style(catalog::text::secondary)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
        )
        .width(Length::Fill)
        .boxed(),
    );
    if page.total > page.page_length() || page.fetching {
        let next = page.page + 1 < page.pages && !page.fetching;
        line = line
            .push(
                action_button(tr!("Previous"), Kind::Ghost, motion)
                    .enabled(page.page > 0)
                    .on_press(Intent::Page(page.page.saturating_sub(1)))
                    .boxed(),
            )
            .push(
                text(tr!("{} of {}", page.page + 1, page.pages))
                    .size(typography::size::SMALL)
                    .style(catalog::text::secondary)
                    .boxed(),
            )
            .push(
                action_button(tr!("Next"), Kind::Ghost, motion)
                    .enabled(next)
                    .on_press(Intent::Page(page.page + 1))
                    .boxed(),
            );
    }
    line.boxed()
}

pub fn view(members: &Members, motion: Motion) -> impl View<Intent> + '_ {
    let mut page = column![top(members, motion), names(members)];
    if members.indexing {
        page = page.push(
            label(tr!(
                "Before searching, we need to index this server. Give us a bit."
            ))
            .boxed(),
        );
    }
    page.push(table(members, motion))
        .push(footer(members, motion))
        .spacing(GAP)
        .height(Length::Fill)
        .width(Length::Fill)
}

/// Its shots for `anomi-shots`: `ssettings/members.png` as the Slint app
/// showed it, then a page of many with the pager and the signals.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::members::{open, open_pages};
    vec![
        Shot {
            area: "ssettings",
            name: "members",
            size: GOLDEN,
            theme: "bonfire",
            vm: open,
        },
        Shot {
            area: "ssettings",
            name: "members_pages",
            size: GOLDEN,
            theme: "bonfire",
            vm: open_pages,
        },
    ]
}
