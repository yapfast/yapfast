//! The new DM and group DM dialog (`NewDmDialog`, `ui/dialogs.slint:137`):
//! a field that finds people, a list of who it found with a "Message" button
//! each, and a line under them for what to type, that one is starting, or
//! why it failed. The shell gives it the dialog's backdrop, Escape and the
//! way in; the card is drawn here.

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::new_dm::{Intent, NewDm};
use anomi_widgets::button::icon_button;
use anomi_widgets::field::{Id, field};
use anomi_widgets::list::List;
use yapfast_lang::tr;

mod person;

/// The card's width and the most it grows to (`ui/dialogs.slint:162`).
const WIDTH: f32 = 440.0;
const TALLEST: f32 = 520.0;
/// `padding: 20px; spacing: 12px`.
const PAD: f32 = 20.0;
/// The title's size (`font-size: 18px`).
const TITLE_SIZE: f32 = 18.0;
const GAP: f32 = 12.0;
/// The title's and the field's height (the title's line, `min-height: 36px`).
const TITLE: f32 = 28.0;
const FIELD: f32 = 36.0;
/// The list's room: what the card has left under the title and the field.
const LIST_MOST: f32 = TALLEST - 2.0 * PAD - TITLE - FIELD - 2.0 * GAP;
/// A row and the gap between rows.
const ROW: f32 = person::HEIGHT;
const ROW_GAP: f32 = 2.0;
/// The note's padding and its line, as it is laid out under the rows.
const NOTE_PAD: f32 = 8.0;
const NOTE_LINE: f32 = 20.0;

/// The field's id: the app focuses it as the dialog opens (`init`,
/// `ui/dialogs.slint:171`) with `anomi_widgets::field::focus`.
pub fn query_id() -> Id {
    Id::new("new-dm-query")
}

pub fn view(new_dm: &NewDm, motion: Motion) -> impl View<Intent> + '_ {
    let title = row![
        text(tr!("New message"))
            .size(TITLE_SIZE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(28.0)
            .glyph(18.0)
            .on_press(Intent::Close)
            .view(),
    ]
    .align_y(Alignment::Center)
    .height(TITLE);
    let placeholder = if new_dm.matrix {
        tr!("Find a friend, or type a Matrix id")
    } else {
        tr!("Find a friend")
    };
    let search = field(placeholder, &new_dm.query)
        .id(query_id())
        .on_input(Intent::Query)
        .text_size(typography::size::BODY)
        .pad(10.0)
        .height(FIELD)
        .view();
    container(
        column![title, search, people(new_dm, motion)]
            .spacing(GAP)
            .padding(PAD),
    )
    .width(WIDTH)
    .style(card)
}

/// The rows and the note, as one list that scrolls once it passes the room
/// the card has; shorter, the card is as tall as they are.
fn people(new_dm: &NewDm, motion: Motion) -> Element<'_, Intent> {
    let mut rows: Vec<(u64, Element<'_, Intent>)> = Vec::new();
    let mut height = 0.0;
    for (index, who) in new_dm.people.iter().enumerate() {
        rows.push((
            index as u64,
            person::row(index, who, new_dm.starting, motion),
        ));
        height += ROW + ROW_GAP;
    }
    if !new_dm.note.is_empty() {
        rows.push((u64::MAX, note(&new_dm.note)));
        height += NOTE_LINE + 2.0 * NOTE_PAD;
    }
    if rows.is_empty() {
        return space().height(0).boxed();
    }
    List::with_rows(rows, motion)
        .spacing(ROW_GAP)
        .height(height.min(LIST_MOST))
        .boxed()
}

fn note(note: &str) -> Element<'_, Intent> {
    container(
        text(note)
            .size(typography::size::SMALL)
            .style(catalog::text::muted),
    )
    .padding(NOTE_PAD)
    .width(Fill)
    .boxed()
}

/// `background: Theme.raised; border: edge 1px; radius 12px;` and the
/// dialog's shadow (32 px blur, 12 px down, `#00000066`).
fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: iced::Color::from_rgba8(0, 0, 0, 0x66 as f32 / 255.0),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}

/// Its shots for `anomi-shots`: the whole window over the sample dialog,
/// the first the golden screenshot's (`dialog/new_dm.png`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::{new_dm, shell};

    use crate::shots::{GOLDEN, Shot};

    fn over(new_dm: NewDm) -> Vm {
        Vm {
            new_dm,
            ..shell::new_dm()
        }
    }
    let shot = |name, vm| Shot {
        area: "dialog",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("new_dm", || over(new_dm::friends())),
        shot("new_dm_found", || over(new_dm::found())),
        shot("new_dm_matrix", || over(new_dm::matrix())),
        shot("new_dm_hint", || over(new_dm::hint())),
        shot("new_dm_nobody", || over(new_dm::nobody())),
        shot("new_dm_starting", || over(new_dm::starting())),
        shot("new_dm_failed", || over(new_dm::failed())),
    ]
}
