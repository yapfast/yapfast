//! The threads popover, under the header's Threads button. Task C23
//! (docs/iced/tasks/chat.md). The Slint side: `ThreadsPopover`
//! (`ui/header.slint:406`), placed at `ui/yapfast.slint:4928`.
//!
//! A 48 px header (the threads icon, "Threads", the name search when the
//! account may read history, Create, Close) over the list: sections of
//! cards, or a note while it loads or failed, or the empty state's
//! invitation to start one. The shell places it and closes it on Escape and
//! a press outside.

mod card;
mod ends;
mod entry;

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{column, container, row, rule, space, stack, text};
use iced::{Alignment, Background, Border, Fill, Padding, Shadow, Vector, border};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::threads::{Intent, Row, ThreadFrom, Threads};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, Side, action_button, icon_button};
use anomi_widgets::field::{self, field};
use anomi_widgets::icon::icon;
use anomi_widgets::list::list;
use yapfast_lang::tr;

/// As Discord's, and `min(max(480, 35 %), 600)` of the 1 400 px window the
/// golden shots show; the height runs from 400 px to 80 % of that window
/// (688 px), as tall as its rows. The shell's layer keeps it in the window.
const WIDTH: f32 = 490.0;
const LEAST: f32 = 400.0;
const MOST: f32 = 688.0;
const RADIUS: f32 = 10.0;
/// The popover's own 1 px edge: the content sits inside it, and the header's
/// ground rounds to the radius less that (`ui/header.slint:434`).
const EDGE: f32 = 1.0;
/// The header: 46 px of panel, then its 1 px rule in the edge colour, which
/// is 48 px from the top with the edge (`ui/header.slint:440-463`).
const PANEL: f32 = 46.0;
/// The shadow under it (`ui/header.slint:434`).
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_OFFSET: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;
/// The header's parts: the threads icon, the gap between them, the inset on
/// the right (the left is `SPACE_L`), the divider before the search, and
/// the Close button with its glyph (`ui/header.slint:440-498`).
const THREADS_ICON: f32 = 20.0;
const HEADER_GAP: f32 = 10.0;
const HEADER_RIGHT: f32 = 10.0;
const DIVIDER: f32 = 24.0;
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The search box (`HeaderSearch`): 28 px tall, 30 with the client style,
/// and as wide as the header leaves it at this width.
const SEARCH: f32 = 228.0;
const SEARCH_HEIGHT: f32 = 30.0;
const SEARCH_PAD: f32 = 8.0;
const SEARCH_RADIUS: f32 = 8.0;
/// Its clear cross: 24 px with a 14 px glyph, `26px` from the right end
/// (`ui/header.slint:137-153`).
const CROSS: f32 = 24.0;
const CROSS_GLYPH: f32 = 14.0;
const CROSS_FROM_RIGHT: f32 = 2.0;
/// The empty state (`ui/header.slint:319-397`): a 48 px icon, a 17 px
/// title, the words and the button, 8 px apart in 32 px of room.
const NOTHING_ICON: f32 = 48.0;
const NOTHING_TITLE: f32 = 17.0;
const NOTHING_GAP: f32 = 8.0;
const NOTHING_PAD: f32 = 32.0;
const NOTHING_BUTTON_TOP: f32 = 8.0;
/// The loading line under the list: 8 px around its text.
const LOADING_PAD: f32 = 8.0;

/// The search field's id, for the app to focus as the popover opens
/// (`autofocus`).
pub fn query_id() -> field::Id {
    field::Id::new("threads-query")
}

pub fn view(threads: &Threads, motion: Motion) -> impl View<Intent> + '_ {
    let body = if !threads.note.is_empty() {
        note(&threads.note)
    } else if threads.empty {
        nothing(threads.can_create, motion)
    } else {
        sections(threads, motion)
    };
    let height = threads.fit().clamp(LEAST, MOST);
    container(column![
        header(threads, motion),
        rule::horizontal(EDGE).style(edge),
        body,
    ])
    .padding(EDGE)
    .width(WIDTH)
    .height(height)
    .clip(true)
    .style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(Background::Color(palette.sidebar)),
            border: Border::default()
                .rounded(RADIUS)
                .width(1)
                .color(palette.edge),
            shadow: Shadow {
                color: iced::Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
                offset: Vector::new(0.0, SHADOW_OFFSET),
                blur_radius: SHADOW_BLUR,
            },
            ..container::Style::default()
        }
    })
}

/// The rule under the header, in the popover's edge colour.
fn edge(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme.palette().edge,
        ..catalog::rule::line(theme)
    }
}

fn header<'a>(threads: &'a Threads, motion: Motion) -> Element<'a, Intent> {
    let mut parts: Vec<Element<'a, Intent>> = vec![
        icon(Icon::Thread, THREADS_ICON)
            .style(|theme: &Theme, _status| iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            })
            .boxed(),
        text(tr!("Threads"))
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .boxed(),
    ];
    if threads.searchable {
        parts.push(
            container(rule::vertical(1).style(catalog::rule::line))
                .height(DIVIDER)
                .boxed(),
        );
        parts.push(search(threads, motion));
    }
    parts.push(space().width(Fill).boxed());
    if threads.can_create {
        parts.push(
            action_button(tr!("Create"), Kind::Primary, motion)
                .small()
                .on_press(Intent::Create(ThreadFrom::List))
                .boxed(),
        );
    }
    parts.push(
        icon_button(Icon::X, tr!("Close"), motion)
            .size(CLOSE)
            .glyph(CLOSE_GLYPH)
            .tip_side(Side::Below)
            .on_press(Intent::Close)
            .view()
            .boxed(),
    );
    container(row(parts).spacing(HEADER_GAP).align_y(Alignment::Center))
        .padding(Padding::from([0.0, HEADER_RIGHT]).left(metrics::SPACE_L))
        .height(PANEL)
        .center_y(PANEL)
        .width(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().panel)),
            border: Border::default().rounded(border::top(RADIUS - EDGE)),
            ..container::Style::default()
        })
        .boxed()
}

/// The name search: the magnifier, which gives way to a clear cross while
/// the query has text.
fn search<'a>(threads: &'a Threads, motion: Motion) -> Element<'a, Intent> {
    let typed = !threads.query.is_empty();
    let input = field(tr!("Search for Thread Name"), &threads.query)
        .id(query_id())
        .on_input(Intent::Edit)
        .on_submit(Intent::Search(threads.query.clone()))
        .text_size(typography::size::SMALL)
        .pad(SEARCH_PAD)
        .height(SEARCH_HEIGHT)
        .radius(SEARCH_RADIUS)
        .trailing(Icon::Search, typed.then_some(iced::Color::TRANSPARENT))
        .motion(motion)
        .view();
    let mut layers: Vec<Element<'a, Intent>> = vec![input];
    if typed {
        layers.push(
            container(
                icon_button(Icon::X, tr!("Clear search"), motion)
                    .size(CROSS)
                    .glyph(CROSS_GLYPH)
                    .tip_side(Side::Below)
                    .on_press(Intent::Edit(String::new()))
                    .view(),
            )
            .padding(Padding::default().right(CROSS_FROM_RIGHT))
            .align_right(Fill)
            .center_y(SEARCH_HEIGHT)
            .boxed(),
        );
    }
    container(stack(layers).width(Fill).height(SEARCH_HEIGHT))
        .width(SEARCH)
        .boxed()
}

/// Said instead of the list.
fn note(words: &str) -> Element<'_, Intent> {
    container(
        text(words)
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
            .align_x(Alignment::Center),
    )
    .padding(metrics::SPACE_XL)
    .center(Fill)
    .boxed()
}

/// No threads: the invitation to start one.
fn nothing<'a>(can_create: bool, motion: Motion) -> Element<'a, Intent> {
    let mut parts: Vec<Element<'a, Intent>> = vec![
        icon(Icon::Thread, NOTHING_ICON)
            .style(|theme: &Theme, _status| iced::widget::svg::Style {
                color: Some(theme.palette().faint),
            })
            .boxed(),
        text(tr!("There are no threads."))
            .size(NOTHING_TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .align_x(Alignment::Center)
            .boxed(),
        text(tr!(
            "Stay focused on a conversation with a thread - a temporary text channel."
        ))
        .size(typography::size::SMALL)
        .style(catalog::text::muted)
        .align_x(Alignment::Center)
        .boxed(),
    ];
    if can_create {
        parts.push(
            container(
                action_button(tr!("Create Thread"), Kind::Primary, motion)
                    .on_press(Intent::Create(ThreadFrom::ListEmpty)),
            )
            .padding(Padding::ZERO.top(NOTHING_BUTTON_TOP))
            .boxed(),
        );
    }
    container(
        column(parts)
            .spacing(NOTHING_GAP)
            .align_x(Alignment::Center),
    )
    .padding(NOTHING_PAD)
    .center(Fill)
    .boxed()
}

/// The threads in their sections, or what a search found, with the
/// loading line under them.
fn sections(threads: &Threads, motion: Motion) -> Element<'_, Intent> {
    let rows = &threads.rows;
    let key = |at: usize| {
        let mut hasher = DefaultHasher::new();
        match &rows[at] {
            Row::Heading(heading) => heading.hash(&mut hasher),
            Row::Thread(thread) => thread.room.hash(&mut hasher),
        }
        hasher.finish()
    };
    let list = list(
        rows.len(),
        key,
        move |at| entry::row_at(rows, at, motion),
        motion,
    )
    .on_viewport(|view| Intent::Scrolled {
        offset: view.offset,
        near_end: view.near_end,
    });
    let loading = threads.loading.then(|| {
        container(
            text(tr!("Loading..."))
                .size(typography::size::LABEL)
                .style(catalog::text::muted),
        )
        .padding(LOADING_PAD)
        .center_x(Fill)
    });
    let mut parts = vec![list.boxed()];
    parts.extend(loading.map(|line| line.boxed()));
    column(parts).boxed()
}

/// Its shots for `anomi-shots`: the golden `thread/list_popover.png`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    vec![Shot {
        area: "thread",
        name: "list_popover",
        size: GOLDEN,
        theme: "bonfire",
        vm: anomi_vm::fixtures::chat::threads::list_popover,
    }]
}
