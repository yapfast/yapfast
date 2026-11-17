//! The quick switcher. Task N10 (docs/iced/tasks/shell.md), porting
//! `QuickSwitcher` (`ui/dialogs.slint:364`) and the glue at
//! `src/ui/callbacks.rs:1280`: a card with a search field, the places
//! (those with mentions, then the lately opened, each under its heading,
//! before a search), and a hint about the search's prefixes. The shell
//! draws its backdrop and closes it on Escape or a press outside; the keys
//! that choose are [`keys`], Enter and a click go ([`Intent::Go`]).

mod keys;
mod row;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{Id, column, container, text};
use iced::{Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::switcher::{Intent, Switcher};
use anomi_widgets::field::field;
use anomi_widgets::list::list;
use yapfast_lang::tr;

use keys::Keys;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The list's id, for the app's `list::operation::jump_to` as an arrow key
/// chooses a place.
pub const LIST: &str = "switcher-list";
/// The search field's id, which the app focuses as the switcher opens
/// (`init => self.focus-input()`).
pub const FIELD: &str = "switcher-field";

/// The card's widest and tallest (`min(570px, ..)`, `min(440px, ..)`).
const WIDTH: f32 = 570.0;
const HEIGHT: f32 = 440.0;
/// The card's padding and the gap between its parts (`padding: 16px;
/// spacing: 12px`).
const PAD: f32 = metrics::SPACE_L;
/// The search field: `min-height: 52px; text-size: 20px; pad: 14px`.
const FIELD_HEIGHT: f32 = 52.0;
const FIELD_SIZE: f32 = typography::size::DIALOG_TITLE;
const FIELD_PAD: f32 = 14.0;

pub fn view(switcher: &Switcher, motion: Motion) -> impl View<Intent> + '_ {
    let current = switcher.current();
    let go = current.map(|at| Intent::Go(switcher.places[at].id.clone()));
    let mut search = field(tr!("Where would you like to go?"), switcher.query.as_str())
        .id(Id::new(FIELD))
        .on_input(Intent::Search)
        .text_size(FIELD_SIZE)
        .pad(FIELD_PAD)
        .height(FIELD_HEIGHT)
        .motion(motion);
    if let Some(go) = go {
        search = search.on_submit(go);
    }
    let search = Keys::new(
        search.view(),
        switcher.next().map(Intent::Choose),
        switcher.previous().map(Intent::Choose),
    );
    let places = list(
        switcher.rows(),
        |at| switcher.entry(at).key(),
        move |at| row::view(switcher, at, current, motion),
        motion,
    )
    .id(Id::new(LIST))
    .estimate(row::PITCH)
    .height(Fill);
    let body: Element<'_, Intent> = if switcher.places.is_empty() && !switcher.query.is_empty() {
        container(
            text(tr!("Nothing goes by that name."))
                .size(typography::size::LABEL)
                .style(catalog::text::muted),
        )
        .padding(metrics::SPACE_S)
        .height(Fill)
        .boxed()
    } else {
        places.boxed()
    };
    let hint = text(tr!(
        "Start a search with @ for people, # for text channels, ! for voice channels or * for servers."
    ))
    .size(typography::size::CAPTION)
    .style(catalog::text::muted);
    // The card's height holds while the list is short, as Slint's does.
    container(
        column![search.boxed(), body, hint]
            .spacing(metrics::SPACE_M)
            .height(Fill),
    )
    .padding(PAD)
    .width(Length::Fixed(WIDTH))
    .height(Length::Fixed(HEIGHT))
    .style(card)
}

/// `background: Theme.raised; border-color: Theme.edge; border-width: 1px;
/// border-radius: 12px; drop-shadow-blur: 32px; drop-shadow-color: #00000066;
/// drop-shadow-offset-y: 12px`.
fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        border: iced::Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: iced::Shadow {
            color: iced::Color::from_rgba8(0, 0, 0, 0.4),
            offset: iced::Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..catalog::container::raised(theme)
    }
}
