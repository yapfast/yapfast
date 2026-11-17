//! Joining a server by an invite, as Discord's Join a Server and invite
//! dialogs do: the server shows before anyone joins. The Slint side:
//! `JoinDialog` (`ui/server.slint:139`).

use iced::widget::Id;

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::places::join::{Intent, Join};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use yapfast_lang::tr;

use super::invite::frame::{buttons, confirm, dialog, failure, found, heading, leave, note};

/// The card's width at least (`:154`).
const WIDTH: f32 = 440.0;
/// The invite field's padding (`pad: 10px`, `ui/server.slint:176`).
const FIELD_PAD: f32 = 10.0;
/// The invite field's id: the app focuses it when the dialog opens with one
/// (`init => { self.focus-input(); }`, `:175`).
pub const LINK: &str = "join-link";
/// The buttons only some states have.
pub const CAPTCHA: &str = "join-captcha";
pub const BROWSER: &str = "join-browser";

pub fn view(join: &Join, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if join.field {
        parts.push(note(tr!(
            "Paste an invite link or code to join a server you were invited to."
        )));
        parts.push(heading(tr!("INVITE LINK")));
        let mut link = field("https://discord.gg/yapfast", &join.link)
            .id(Id::new(LINK))
            .text_size(typography::size::BODY)
            .pad(FIELD_PAD)
            .motion(motion)
            .on_input(Intent::Link);
        if !join.busy {
            link = link.on_submit(Intent::Find(join.link.clone()));
        }
        parts.push(link.view());
    }
    if let Some(server) = &join.found {
        parts.push(found(Some(&server.avatar), &server.name, &[&server.detail]));
    }
    if !join.status.is_empty() {
        parts.push(note(&join.status));
    }
    if join.risk {
        parts.push(note(tr!(
            "Discord can take a join made from an app other than its own for suspicious and sign you out on every device. Joining in your browser avoids that."
        )));
    }
    if !join.error.is_empty() {
        parts.push(failure(&join.error));
    }
    let mut ends = vec![leave(join.closes_only(), Intent::Close, motion)];
    if join.captcha {
        ends.push(
            action_button(tr!("Open the captcha again"), Kind::Secondary, motion)
                .id(Id::new(CAPTCHA))
                .on_press(Intent::OpenCaptcha)
                .boxed(),
        );
    }
    if join.risk {
        ends.push(
            action_button(tr!("Join in browser"), Kind::Secondary, motion)
                .id(Id::new(BROWSER))
                .enabled(!join.busy)
                .on_press(Intent::JoinInBrowser)
                .boxed(),
        );
    }
    if !join.confirm.is_empty() {
        ends.push(confirm(&join.confirm, join.busy, join.confirmed(), motion));
    }
    parts.push(buttons(ends));
    dialog(&join.title, WIDTH, Intent::Close, motion, parts)
}

#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::places::join;
    vec![Shot {
        area: "dialog",
        name: "add_server",
        size: GOLDEN,
        theme: "bonfire",
        vm: join::adding,
    }]
}
