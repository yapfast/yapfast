//! The Requests screen: friend requests and message requests, a person's
//! together, then those this account sent, and spam folded away, in place of
//! a conversation. Task P02 (docs/iced/tasks/people.md). The Slint side:
//! `RequestsScreen` and `RequestsHeading` (`ui/friends.slint:345`, `:150`);
//! the rows are [`item`], their message [`message`]. The handlers are at
//! `src/ui/callbacks.rs:979`, `:1062`.

mod item;
mod message;
mod shots;

use iced::widget::{column, container, hover, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::people::requests::{Intent, Requests};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The list's padding (`ui/friends.slint:362`) and the gap between rows.
const PAD: f32 = metrics::SPACE_L;
const TOP: f32 = 8.0;
const GAP: f32 = 2.0;
/// A heading: 36 px tall, on the rows' 10 px inset (`:150`), its words 12 px.
const HEADING: f32 = 36.0;
const INSET: f32 = 10.0;
const HEADING_TEXT: f32 = typography::size::CAPTION;
/// The spam's chevron (`:393`).
const CHEVRON: f32 = 14.0;
const CHEVRON_GAP: f32 = 4.0;
/// The empty screen: its disc, glyph and title (`:354`-`:359`).
const EMPTY_PAD: f32 = 32.0;
const EMPTY_GAP: f32 = 10.0;
const DISC: f32 = 80.0;
const DISC_GLYPH: f32 = 36.0;
const EMPTY_TITLE: f32 = 22.0;

pub fn view(requests: &Requests, motion: Motion) -> impl View<Intent> + '_ {
    let screen: Element<'_, Intent> = if requests.is_empty() {
        empty()
    } else {
        listing(requests, motion)
    };
    container(screen).width(Fill).height(Fill).clip(true)
}

/// Both looks of something that changes under the pointer, as iced's `hover`
/// lays one over the other: the real controls are only in the one under it.
pub(super) fn hovering<'a>(make: impl Fn(bool) -> Element<'a, Intent>) -> Element<'a, Intent> {
    hover(make(false), make(true)).boxed()
}

/// Nothing waits.
fn empty<'a>() -> Element<'a, Intent> {
    container(
        column![
            container(icon(Icon::Inbox, DISC_GLYPH).style(|theme: &Theme, _| {
                iced::widget::svg::Style {
                    color: Some(theme.palette().secondary),
                }
            }))
            .center(DISC)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().raised)),
                border: Border::default().rounded(DISC / 2.0),
                ..container::Style::default()
            }),
            text(tr!("No requests"))
                .size(EMPTY_TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary),
            text(tr!(
                "Friend requests and messages from people you don't know show up here."
            ))
            .style(catalog::text::secondary)
            .align_x(Alignment::Center),
        ]
        .spacing(EMPTY_GAP)
        .align_x(Alignment::Center),
    )
    .padding(EMPTY_PAD)
    .center(Fill)
    .boxed()
}

/// The headings, rows and the spam's fold.
fn listing(requests: &Requests, motion: Motion) -> Element<'_, Intent> {
    let mut rows: Vec<Element<'_, Intent>> = Vec::new();
    for (heading, list) in [
        (
            tr!("REQUESTS — {}", requests.incoming.len()),
            &requests.incoming,
        ),
        (tr!("SENT — {}", requests.sent.len()), &requests.sent),
    ] {
        if list.is_empty() {
            continue;
        }
        rows.push(heading_of(heading));
        rows.extend(
            list.iter()
                .map(|request| item::view(request, false, requests.marks, motion)),
        );
    }
    if !requests.spam.is_empty() {
        rows.push(fold(requests.spam.len(), requests.spam_open));
        if requests.spam_open {
            rows.extend(
                requests
                    .spam
                    .iter()
                    .map(|request| item::view(request, true, requests.marks, motion)),
            );
        }
    }
    scroll_view(
        column(rows)
            .spacing(GAP)
            .width(Fill)
            .padding(Padding::new(PAD).top(TOP)),
        motion,
    )
    .boxed()
}

/// A heading, on the rows' inset so it starts where their avatars do.
fn heading_of<'a>(words: String) -> Element<'a, Intent> {
    container(
        text(words)
            .size(HEADING_TEXT)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted),
    )
    .padding(Padding::ZERO.left(INSET))
    .height(HEADING)
    .center_y(HEADING)
    .boxed()
}

/// The spam's heading, which folds and unfolds it: its words and chevron
/// are muted, and secondary under the pointer.
fn fold<'a>(count: usize, open: bool) -> Element<'a, Intent> {
    hovering(move |hovered| {
        let ink = move |theme: &Theme| {
            let palette = theme.palette();
            if hovered {
                palette.secondary
            } else {
                palette.muted
            }
        };
        let words = text(tr!("SPAM — {}", count))
            .size(HEADING_TEXT)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink(theme)),
                ..text::Style::default()
            });
        let chevron = icon(
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            CHEVRON,
        )
        .style(move |theme: &Theme, _| iced::widget::svg::Style {
            color: Some(ink(theme)),
        });
        // Each centred on its own in the band, as `heading_of` centres its text.
        let heading = container(
            iced::widget::row![
                container(words).height(HEADING).center_y(HEADING),
                container(chevron).height(HEADING).center_y(HEADING),
            ]
            .spacing(CHEVRON_GAP),
        )
        .padding(Padding::ZERO.left(INSET))
        .height(HEADING);
        if hovered {
            iced::widget::button(heading)
                .padding(0)
                .width(Fill)
                .on_press(Intent::ToggleSpam)
                .style(|theme: &Theme, _| iced::widget::button::Style {
                    // Covers the resting copy under it.
                    background: Some(Background::Color(theme.palette().chat)),
                    ..iced::widget::button::Style::default()
                })
                .boxed()
        } else {
            heading.width(Fill).boxed()
        }
    })
}
