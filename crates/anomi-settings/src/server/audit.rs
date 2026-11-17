//! A server's audit log: who did what, newest first, with filters by user
//! and by action. A row's arrow opens its numbered lines; older entries load
//! at the end of the list. Task X07's (docs/iced/tasks/settings.md). The
//! Slint side: `AuditLogSection` and `AuditFilter`
//! (`ui/moderation.slint:330`, `:311`).

mod head;

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::audit::{Audit, Intent, Kind, Line, Row};
use anomi_widgets::avatar::circle;
use anomi_widgets::button::{Kind as Look, action_button};
use anomi_widgets::list::list;
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

use crate::kit::{heading, picker};
use head::{ARROW, head};

const GAP: f32 = 16.0;
const FILTER: f32 = 8.0;
const PAD: f32 = 12.0;
const AVATAR: f32 = 40.0;
/// An item's indent under its line, where the number stands on a line.
const INDENT: f32 = 52.0;
/// The empty log's title: 17 px bold.
const EMPTY_TITLE: f32 = 17.0;

pub fn view(audit: &Audit, motion: Motion) -> impl View<Intent> + '_ {
    let filters = row![
        filter(
            tr!("Filter by User"),
            &audit.users,
            audit.user,
            audit.fetching,
            Intent::User
        ),
        filter(
            tr!("Filter by Action"),
            &audit.actions,
            audit.action,
            audit.fetching,
            Intent::Action
        ),
    ]
    .spacing(GAP);
    let mut page = column![filters, rule()].spacing(GAP).height(Length::Fill);
    if audit.rows.is_empty() {
        page = page.push(empty(audit));
    } else {
        page = page.push(rows(audit, motion));
        if audit.older {
            page = page.push(
                container(
                    action_button(
                        if audit.fetching {
                            tr!("Loading...")
                        } else {
                            tr!("Load More")
                        },
                        Look::Secondary,
                        motion,
                    )
                    .on_press(Intent::Older)
                    .enabled(!audit.fetching),
                )
                .center_x(Length::Fill)
                .boxed(),
            );
        }
    }
    page
}

/// A label over a drop-down; Discord's client ignores a filter while it
/// fetches, so the picker is off then.
fn filter<'a>(
    label: &'a str,
    options: &'a [String],
    chosen: usize,
    fetching: bool,
    on_pick: fn(usize) -> Intent,
) -> Element<'a, Intent> {
    column![
        heading(label),
        picker(options, chosen, (!fetching).then_some(on_pick)),
    ]
    .spacing(FILTER)
    .width(Length::Fill)
    .boxed()
}

/// What stands in for the rows: the wait at the top (Slint's `Label`), or
/// why there are none, in the middle.
fn empty(audit: &Audit) -> Element<'_, Intent> {
    if audit.fetching {
        return container(
            text(tr!("Loading..."))
                .size(typography::size::LABEL)
                .style(catalog::text::muted),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed();
    }
    let (title, body) = if audit.failed {
        (
            tr!("This is Awkward"),
            tr!("We broke something. Come back later."),
        )
    } else {
        (
            tr!("No Logs Yet"),
            tr!("Once moderators begin moderating, you can moderate the moderation here."),
        )
    };
    container(
        column![
            text(title)
                .size(EMPTY_TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary)
                .align_x(Alignment::Center),
            text(body)
                .size(typography::size::SMALL)
                .style(catalog::text::secondary)
                .align_x(Alignment::Center),
        ]
        .spacing(FILTER)
        .align_x(Alignment::Center)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .center_y(Length::Fill)
    .boxed()
}

fn rows(audit: &Audit, motion: Motion) -> Element<'_, Intent> {
    let mut list = list(
        audit.rows.len(),
        |i| i as u64,
        move |i| entry(&audit.rows[i], i, audit.open == Some(i), motion),
        motion,
    )
    .height(Length::Fill);
    // The end of the list loads older entries, as Discord's does.
    if audit.wants_older() {
        list = list.on_viewport(|view| Intent::Scrolled {
            near_end: view.near_end,
        });
    }
    list.boxed()
}

/// A row: its head, and under it, when open, its lines, in a card.
fn entry(row: &Row, index: usize, open: bool, motion: Motion) -> Element<'_, Intent> {
    let mut card = column![top(row, index, open, motion)];
    if open {
        card = card.push(rule()).push(lines(row));
    }
    container(container(card).style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(palette.lift(0.02).into()),
            border: Border {
                color: palette.line,
                width: 1.0,
                radius: metrics::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        }
    }))
    .padding(Padding {
        bottom: 8.0,
        ..Padding::ZERO
    })
    .width(Length::Fill)
    .boxed()
}

fn rule<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Length::Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().line.into()),
            ..container::Style::default()
        })
        .boxed()
}

/// Who, what and when, and the arrow when the row opens.
fn top(row: &Row, index: usize, open: bool, motion: Motion) -> Element<'_, Intent> {
    let words = column![
        paragraph(&row.title)
            .size(typography::size::BODY)
            .color(|palette| palette.secondary)
            .max_lines(1),
        text(&row.time)
            .size(typography::size::LABEL)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .spacing(2)
    .width(Length::Fill);
    let mut line = row![circle(&row.actor, AVATAR), words]
        .spacing(PAD)
        .padding(PAD)
        .align_y(Alignment::Center);
    if row.expandable() {
        line = line.push(space().width(ARROW).boxed());
    }
    let on_press = row.expandable().then_some(Intent::Toggle(index));
    head(line.boxed(), on_press, open, PAD, motion)
}

/// The open row's numbered lines, and the items under them.
fn lines(row: &Row) -> Element<'_, Intent> {
    column(row.lines.iter().map(|line| numbered(line, row.kind)))
        .spacing(4)
        .padding(PAD)
        .boxed()
}

fn numbered(line: &Line, kind: Kind) -> Element<'_, Intent> {
    if line.number.is_empty() {
        return container(
            paragraph(&line.text)
                .size(typography::size::LABEL)
                .color(|palette| palette.muted)
                .width(Length::Fill),
        )
        .padding(Padding {
            left: INDENT,
            ..Padding::ZERO
        })
        .boxed();
    }
    row![
        text(format!("{}\u{2014}", line.number))
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .style(move |theme: &Theme| {
                let palette = theme.palette();
                text::Style {
                    color: Some(match kind {
                        Kind::Create => palette.live,
                        Kind::Update => palette.accent,
                        Kind::Delete => palette.danger,
                    }),
                    ..text::Style::default()
                }
            }),
        paragraph(&line.text)
            .size(typography::size::SMALL)
            .color(|palette| palette.secondary)
            .width(Length::Fill),
    ]
    .spacing(6)
    .boxed()
}

/// Its shots for `anomi-shots`: the log, a row open, waiting, empty, and
/// failed.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::audit::{empty, failed, loading, open, page};
    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "ssettings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("audit_log", page),
        shot("audit_log_open", open),
        shot("audit_log_loading", loading),
        shot("audit_log_empty", empty),
        shot("audit_log_failed", failed),
    ]
}
