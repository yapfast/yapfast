//! The new DM and group DM dialog (`NewDmDialog`, `ui/dialogs.slint:137`):
//! a field that finds people, a list of who it found with a "Message" button
//! each, and a line under them for what to type, that one is starting, or
//! why it failed. The shell gives it the dialog's backdrop, Escape and the
//! way in; the card is drawn here.

use iced::widget::{column, container, mouse_area, row, space, text};
use iced::{Alignment, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::new_dm::{Intent, NewDm};
use anomi_widgets::button::icon_button;
use anomi_widgets::field::{Id, field};
use anomi_widgets::list::list;
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
/// The gap between rows, and the note's padding.
const ROW_GAP: f32 = 2.0;
const NOTE_PAD: f32 = 8.0;

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
    let card = container(
        column![title, search, people(new_dm, motion)]
            .spacing(GAP)
            .padding(PAD),
    )
    .width(WIDTH)
    .style(card);
    // The layer centres this box, so the card's top stays where Slint keeps
    // it (`ui/dialogs.slint:153`) and the card grows downwards; a press
    // under the card closes, as the backdrop does there.
    column![
        card,
        mouse_area(space().width(Fill).height(Fill)).on_press(Intent::Close)
    ]
    .width(WIDTH)
    .height(TALLEST)
}

/// The rows and the note, as one list that shrinks to them and scrolls once
/// they pass the room the card has.
fn people(new_dm: &NewDm, motion: Motion) -> Element<'_, Intent> {
    let count = new_dm.people.len() + usize::from(!new_dm.note.is_empty());
    if count == 0 {
        return space().height(0).boxed();
    }
    list(
        count,
        |index| index as u64,
        move |index| match new_dm.people.get(index) {
            Some(who) => person::row(index, who, new_dm.starting, motion),
            None => note(&new_dm.note),
        },
        motion,
    )
    .spacing(ROW_GAP)
    .height(Length::Shrink)
    .boxed()
}

fn note(note: &str) -> Element<'_, Intent> {
    container(
        text(note)
            .size(typography::size::LABEL)
            .style(catalog::text::muted),
    )
    .padding(NOTE_PAD)
    .width(Fill)
    .boxed()
}

/// `background: Theme.raised; border: edge 1px; radius 12px;` and the
/// dialog's shadow.
fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: catalog::float::dialog(theme).shadow,
        ..catalog::container::raised(theme)
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
