//! A server's Invites page: the actions, then the table of invites, each
//! revoked at once. Task X04's (docs/iced/tasks/settings.md). The Slint side:
//! `InvitesSection` and `Countdown` (`ui/server_people.slint:295`, `:283`).
//! The countdowns are the view-model's `ticks`: the app adds one a second
//! while the window is in front and `wants_ticks`, so a window behind another
//! ticks nothing and is right again at the first tick back.

mod table;

use iced::Length;
use iced::widget::{column, row, text};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::invites::{Intent, Invites};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use crate::kit::heading;

const GAP: f32 = 16.0;
const TIGHT: f32 = 12.0;
const BUTTONS: f32 = 8.0;

pub fn view(invites: &Invites, motion: Motion) -> impl View<Intent> + '_ {
    let mut actions = row![].spacing(BUTTONS);
    if invites.may_create {
        actions = actions.push(
            action_button(
                tr!("Create Invite Link"),
                if invites.paused {
                    Kind::Secondary
                } else {
                    Kind::Primary
                },
                motion,
            )
            .on_press(Intent::Create),
        );
    }
    actions = actions.push(
        action_button(
            if invites.paused {
                tr!("Enable Invites")
            } else {
                tr!("Pause Invites")
            },
            if invites.paused {
                Kind::Primary
            } else {
                Kind::Secondary
            },
            motion,
        )
        .on_press(Intent::SecurityActions),
    );
    let top = column![
        text(tr!(
            "Create an invite link anyone can use to join, or invite friends directly."
        ))
        .size(typography::size::LABEL)
        .style(catalog::text::muted),
        // Wrapping as the Slint page's FlexboxLayout does in a narrow window.
        actions.wrap()
    ]
    .spacing(TIGHT);

    let mut page = column![top].spacing(GAP).height(Length::Fill);
    if invites.rows.is_empty() {
        page = page.push(heading(tr!("NO ACTIVE INVITE LINKS")).boxed());
    } else {
        page = page.push(table::head()).push(table::rows(invites, motion));
    }
    page
}

/// Its shots for `anomi-shots`: the table, paused, and with none.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::invites::{empty, page, paused};
    vec![
        Shot {
            area: "ssettings",
            name: "invites",
            size: GOLDEN,
            theme: "bonfire",
            vm: page,
        },
        Shot {
            area: "ssettings",
            name: "invites_paused",
            size: GOLDEN,
            theme: "bonfire",
            vm: paused,
        },
        Shot {
            area: "ssettings",
            name: "invites_empty",
            size: GOLDEN,
            theme: "bonfire",
            vm: empty,
        },
    ]
}
