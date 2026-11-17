//! The popover's rows: a `MenuItem` for each (`ui/controls.slint:1156`)
//! over the `Row`s the popover keeps, so they take the keyboard's focus,
//! fade as they light and turn their arrow as the foundation's do; the
//! status row with its dot, and the four statuses (`StatusOption`,
//! `ui/dialogs.slint:567`, P10's).

use iced::widget::{column, container, stack, svg};
use iced::{Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme};
use anomi_vm::account::{Choice, Intent, Popover};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Row;
use anomi_widgets::icon::icon;
use anomi_widgets::menu::item;
use yapfast_lang::tr;

use super::ringed::dot;

/// The status dot's width, and how far in it sits: where an icon would, a
/// `MenuItem`'s padding in (`account-popover.slint:121`).
const DOT: f32 = 16.0;
const DOT_X: f32 = 8.0;
/// The status row's arrow: 16 px, muted, 8 px in from the end
/// (`account-popover.slint:123`).
const ARROW: f32 = 16.0;
const ARROW_X: f32 = 8.0;
/// A `MenuItem` here is 40 px (`height: 40px`), the foundation's row 32.
const ROW: f32 = 40.0;
/// The unfolded options' spacing (`account-popover.slint:139`).
const OPTIONS_GAP: f32 = 2.0;
/// The line between the groups' rows (`height: 1px`).
const RULE: f32 = 1.0;
/// The line between the groups of a card (`height: 1px; Theme.line`).
pub fn rule<'a>() -> impl View<Intent> + 'a {
    container(iced::widget::space())
        .width(Fill)
        .height(RULE)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().line.into()),
            ..container::Style::default()
        })
}

/// A row of its own: its icon and words, sending its intent.
pub fn plain(row: &Row<Intent>, motion: Motion) -> impl View<Intent> + '_ {
    item(row, motion).height(ROW)
}

/// The row that shows the chosen status and unfolds the others; its arrow
/// points down while they show. Its dot sits over the room the row keeps
/// for an icon, its ring the raised ground. The arrow is drawn here, small
/// and muted as Slint's (the foundation's is 24 px in the text's ink), and
/// swapped rather than turned.
pub fn status(popover: &Popover, motion: Motion) -> impl View<Intent> + '_ {
    let ring = container(dot(popover.choice.presence(), DOT, false))
        .padding(Padding::default().left(DOT_X))
        .center_y(Fill);
    let chevron = if popover.expanded {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    let arrow = icon(chevron, ARROW).style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().muted),
    });
    let arrow = container(arrow)
        .padding(Padding::default().right(ARROW_X))
        .align_right(Fill)
        .center_y(Fill);
    stack![
        item(&popover.rows.status, motion)
            .arrow(false)
            .body()
            .height(ROW)
            .boxed(),
        ring.boxed(),
        arrow.boxed()
    ]
    .width(Fill)
}

/// What the status row says of a choice.
pub fn name(choice: Choice) -> &'static str {
    match choice {
        Choice::Online => tr!("Online"),
        Choice::Idle => tr!("Idle"),
        Choice::DoNotDisturb => tr!("Do Not Disturb"),
        Choice::Invisible => tr!("Invisible"),
    }
}

/// The four statuses, the chosen one ticked: P10's `StatusOption`.
pub fn options<'a>(popover: &'a Popover) -> impl View<Intent> + 'a {
    let rows = Choice::ALL.map(|choice| {
        let note = match choice {
            Choice::DoNotDisturb => tr!("You won't get message notifications."),
            Choice::Invisible => tr!("You'll appear offline."),
            _ => "",
        };
        anomi_people::status::option(
            choice,
            name(choice),
            note,
            popover.choice == choice,
            Intent::Choose(choice),
        )
    });
    column(rows).spacing(OPTIONS_GAP)
}
