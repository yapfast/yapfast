//! The confirmation before leaving for a link whose text shows something
//! other than where it goes: Slint's `LeavingDialog` (`ui/dialogs.slint:518`).
//! "Leaving Yapfast", "This link goes to", the site in bold, the whole link
//! in a sunken box that breaks anywhere, then "Go back" and "Visit site".
//! Enter visits, Escape and a press outside go back (the shell's layer).

use iced::widget::{container, text};
use iced::{Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::places::leaving_link::{Intent, LeavingLink};
use anomi_widgets::button::Kind;
use yapfast_lang::tr;

use super::leave::{buttons, dialog, enter, title};

/// The card's width (`min(440px, ..)`, `:531`).
const WIDTH: f32 = 440.0;
/// The site (`font-size: 17px; font-weight: 700`).
const SITE: f32 = 17.0;
/// The link box's padding: 8 px above and below, 10 px beside (`:545`).
const BOX_PADDING: [f32; 2] = [8.0, 10.0];

pub fn view(link: &LeavingLink, motion: Motion) -> impl View<Intent> + '_ {
    let parts = vec![
        title(tr!("Leaving Yapfast"), text::Wrapping::Word).boxed(),
        text(tr!("This link goes to"))
            .size(typography::size::BODY)
            .style(catalog::text::secondary)
            .boxed(),
        text(&link.site)
            .size(SITE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill)
            .boxed(),
        container(
            text(&link.link)
                .size(typography::size::SMALL)
                .style(catalog::text::secondary)
                .width(Fill)
                .wrapping(text::Wrapping::Glyph),
        )
        .padding(BOX_PADDING)
        .width(Fill)
        .style(sunken)
        .boxed(),
        buttons(
            (tr!("Go back"), Intent::Close),
            (tr!("Visit site"), Kind::Primary, Intent::Visit),
            true,
            motion,
        ),
    ];
    enter(dialog(WIDTH, parts), Some(Intent::Visit))
}

/// The box the link sits in: sunken, 6 px corners.
fn sunken(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().sunken.into()),
        border: Border::default().rounded(metrics::RADIUS_ROW),
        ..container::Style::default()
    }
}

/// Its shots for `anomi-shots`: no golden shot exists, so it is compared
/// with the Slint app's dialog under `scripts/ui-session`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    vec![Shot {
        area: "dialog",
        name: "leaving_link",
        size: GOLDEN,
        theme: "bonfire",
        vm: anomi_vm::fixtures::places::leaving_link::asked,
    }]
}
