//! A forum's list of posts: the search of their titles and "New Post" on
//! top, then the cards, the active ones first and older ones a page at a
//! time as the list scrolls, or what a search found; or a note, or the
//! empty state. The Slint side: `ForumPanel`'s list half (`ui/forum.slint:542`-
//! `:598`).

mod clear;

use iced::widget::{column, container, row, stack, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::forum::{Forum, Intent};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::field::{Id, field};
use anomi_widgets::icon::icon;
use anomi_widgets::list::list;
use anomi_widgets::tip::Side;
use yapfast_lang::tr;

use super::card;
use clear::OnClear;

/// The bar over the list is 60 px: the controls 36 px tall, 16 px from the
/// top and the sides (`:544`-`:553`).
const BAR_SIDE: f32 = 16.0;
const BAR_BELOW: f32 = 8.0;
const CONTROL: f32 = 36.0;
/// The search field's text and its inset (`HeaderSearch`, `ui/header.slint:101`).
const SEARCH_PAD: f32 = 8.0;
/// The empty state's picture and the padding around notes (`:562`-`:572`).
const EMPTY_ICON: f32 = 48.0;
const NOTE_PAD: f32 = 24.0;
const EMPTY_PAD: f32 = 32.0;
const EMPTY_TITLE: f32 = 17.0;
/// The search well's corners with the client style (`border-radius`,
/// `ui/header.slint:91`).
const SEARCH_GLYPH: f32 = 20.0;
const SEARCH_RADIUS: f32 = 8.0;
/// The cross that ends a search: 24 px with a 14 px glyph, `26px` from the
/// right end (`x: root.width - 26px`, `ui/header.slint:147`-`:151`).
const CROSS: f32 = 24.0;
const CROSS_GLYPH: f32 = 14.0;
const CROSS_FROM_RIGHT: f32 = 2.0;
/// The id of the search field, for a focus task.
pub const SEARCH: &str = "forum-search";

pub(super) fn view<'a>(forum: &'a Forum, motion: Motion) -> Element<'a, Intent> {
    let mut parts = column![].width(Fill).height(Fill);
    if forum.searchable || forum.can_post {
        parts = parts.push(bar(forum, motion));
    }
    parts = parts.push(body(forum, motion));
    if forum.note.is_empty() && !forum.empty && forum.loading {
        parts = parts.push(
            container(
                text(tr!("Loading..."))
                    .size(typography::size::LABEL)
                    .style(catalog::text::muted),
            )
            .padding(metrics::SPACE_S)
            .center_x(Fill)
            .boxed(),
        );
    }
    parts.boxed()
}

/// The search and "New Post" (`:543`-`:560`).
fn bar<'a>(forum: &'a Forum, motion: Motion) -> Element<'a, Intent> {
    let mut line = row![].spacing(metrics::SPACE_S).align_y(Alignment::Center);
    if forum.searchable {
        line = line.push(search(forum, motion));
    } else {
        line = line.push(iced::widget::space().width(Fill).boxed());
    }
    if forum.can_post {
        line = line.push(
            action_button(tr!("New Post"), Kind::Primary, motion)
                .leading(Icon::Thread)
                .height(CONTROL)
                .on_press(Intent::NewPost)
                .boxed(),
        );
    }
    container(line)
        .padding(iced::Padding {
            top: BAR_SIDE,
            right: BAR_SIDE,
            bottom: BAR_BELOW,
            left: BAR_SIDE,
        })
        .width(Fill)
        .boxed()
}

/// The field: Enter searches; once it holds text, a cross in the place of
/// the magnifier clears it and shows the posts again.
fn search<'a>(forum: &'a Forum, motion: Motion) -> Element<'a, Intent> {
    let well = field(tr!("Search posts"), &forum.query)
        .on_input(Intent::Query)
        .on_submit(Intent::Search(forum.query.clone()))
        .id(Id::new(SEARCH))
        .text_size(typography::size::SMALL)
        .pad(SEARCH_PAD)
        .height(CONTROL)
        .radius(SEARCH_RADIUS)
        .motion(motion);
    let typed = !forum.query.is_empty();
    let input = well.view();
    let mut layers: Vec<Element<'a, Intent>> = vec![input];
    if !typed {
        // The magnifier, 20 px at 8 px from the end (`ui/header.slint:141`).
        layers.push(
            container(icon(Icon::Search, SEARCH_GLYPH).style(|theme: &Theme, _| {
                iced::widget::svg::Style {
                    color: Some(theme.palette().secondary),
                }
            }))
            .padding(Padding::default().right(SEARCH_PAD))
            .align_right(Fill)
            .center_y(CONTROL)
            .boxed(),
        );
    }
    if typed {
        layers.push(
            container(
                icon_button(Icon::X, tr!("Clear search"), motion)
                    .size(CROSS)
                    .glyph(CROSS_GLYPH)
                    .tip_side(Side::Below)
                    .on_press(Intent::Clear)
                    .view(),
            )
            .padding(Padding::default().right(CROSS_FROM_RIGHT))
            .align_right(Fill)
            .center_y(CONTROL)
            .boxed(),
        );
    }
    let content = stack(layers).width(Fill).height(CONTROL).boxed();
    OnClear::new(content, typed.then_some(Intent::Clear)).boxed()
}

/// A note, the empty state, or the cards (`:561`-`:593`).
fn body<'a>(forum: &'a Forum, motion: Motion) -> Element<'a, Intent> {
    if !forum.note.is_empty() {
        return container(
            text(forum.note.as_str())
                .size(typography::size::SMALL)
                .style(catalog::text::muted)
                .align_x(Alignment::Center),
        )
        .padding(NOTE_PAD)
        .center(Fill)
        .boxed();
    }
    if forum.empty {
        let said = if forum.searching {
            tr!("No results found")
        } else {
            tr!("There are no posts yet.")
        };
        return container(
            column![
                icon(Icon::Thread, EMPTY_ICON).style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().faint),
                }),
                text(said)
                    .size(EMPTY_TITLE)
                    .font(typography::BOLD)
                    .style(catalog::text::primary)
                    .align_x(Alignment::Center),
            ]
            .spacing(metrics::SPACE_S)
            .align_x(Alignment::Center),
        )
        .padding(EMPTY_PAD)
        .center(Fill)
        .boxed();
    }
    let rows = &forum.rows;
    let cards = list(
        rows.len(),
        move |at| rows[at].key(),
        move |at| card::view(&rows[at], at == 0, motion),
        motion,
    )
    .on_viewport(|view| Intent::Scrolled {
        near_end: view.near_end,
        by_hand: view.offset > 0.0,
        first: view.first,
        last: view.last,
    });
    container(cards).width(Fill).height(Fill).boxed()
}
