//! A server's or channel's invite dialog, Discord's two pages in one card:
//! whom to send the invite to, with the link to copy at its foot
//! ([`people`]), and how long a new link lasts and how many may use it
//! ([`settings`]). The Slint side: `InviteDialog` (`ui/server.slint:27`),
//! which had the second page alone.

mod choices;
pub(crate) mod frame;
mod people;
mod rows;
mod settings;

use anomi_theme::prelude::*;
use anomi_vm::places::invite::{Intent, Invite, Page};
use yapfast_lang::tr;

use choices::natural;
use frame::PADDING;

pub use people::{LIST, SEARCH};
pub use settings::REMAKE;

/// The card's width at least (`:44`).
const WIDTH: f32 = 460.0;

pub fn view(invite: &Invite, motion: Motion) -> impl View<Intent> + '_ {
    let width = width();
    match invite.page {
        Page::People => people::view(invite, width, motion),
        Page::Link => settings::view(invite, width, motion),
    }
}

/// How long a link may last, and how often it works, as the troughs say.
fn ages() -> [&'static str; 7] {
    [
        tr!("30 min"),
        tr!("1 hr"),
        tr!("6 hr"),
        tr!("12 hr"),
        tr!("1 day"),
        tr!("7 days"),
        tr!("Never"),
    ]
}

fn uses() -> [&'static str; 7] {
    [tr!("No limit"), "1", "5", "10", "25", "50", "100"]
}

/// One width for both pages, so the card keeps its place as they turn: it
/// fits the troughs, inside its padding (`:44`), and the second page's
/// buttons.
fn width() -> f32 {
    frame::fit(
        WIDTH.max(natural(&ages()).max(natural(&uses())) + 2.0 * PADDING),
        &[tr!("Back"), tr!("Generate a new link")],
    )
}

#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, SMALL, Shot};
    use anomi_vm::fixtures::places::invite;
    let shot = |name: &'static str, vm: fn() -> anomi_vm::Vm| Shot {
        area: "dialog",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("invite_to_server", invite::made),
        shot("invite_voice", || invite::over(invite::voice())),
        shot("invite_nobody", || invite::over(invite::nobody())),
        shot("invite_settings", || invite::over(invite::settings())),
        // The list shrinks in a small window, and the foot stays.
        Shot {
            size: SMALL,
            ..shot("invite_small", || invite::over(invite::voice()))
        },
    ]
}
