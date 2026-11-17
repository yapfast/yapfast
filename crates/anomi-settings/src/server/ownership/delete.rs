//! Delete Server: `DeleteServerDialog` (`ui/server_ownership.slint:69`).

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::settings::ownership::{Intent, Ownership};
use yapfast_lang::tr;

use super::NAME;
use super::card::{buttons, card, problem, title, typed, words};

pub fn view(ownership: &Ownership, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts = vec![title(&ownership.title), words(ownership)];
    if ownership.by_name {
        let wrong = ownership
            .wrong
            .then_some(tr!("You didn't enter the server name correctly"));
        parts.push(typed(
            tr!("ENTER SERVER NAME"),
            NAME,
            ownership,
            wrong,
            motion,
        ));
    }
    if !ownership.error.is_empty() {
        parts.push(problem(&ownership.error, typography::size::SMALL));
    }
    let confirm = if ownership.sending {
        tr!("Deleting...")
    } else {
        tr!("Delete Server")
    };
    parts.push(buttons(confirm, !ownership.sending, ownership, motion));
    card(parts)
}
